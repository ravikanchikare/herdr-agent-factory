//! Process lifecycle and opaque byte transport for the Rust application core.
//!
//! This module knows the IPC framing contract, but no application methods or
//! domain concepts. A frame is UTF-8 JSON prefixed by a four-byte big-endian
//! length. Requests and responses correlate by their transport `id`; all other
//! frames are retained in a bounded event queue for diagnostics and later UI
//! resynchronization.

const std = @import("std");
const native_sdk = @import("native_sdk");
const bundle_resources = @import("bundle_resources.zig");

pub const protocol_version: i64 = 1;
pub const max_frame_bytes: usize = 1024 * 1024;
pub const max_pending_requests: usize = 64;
pub const max_cached_events: usize = 64;
pub const max_cached_event_bytes: usize = 512 * 1024;
const max_id_bytes: usize = native_sdk.bridge.max_id_bytes;
const max_kind_bytes: usize = 16;
const max_stderr_bytes: usize = 16 * 1024;
const shutdown_grace_ms: u64 = 2_000;
const shutdown_poll_ms: u64 = 25;
const runtime_path_env = "AGENT_FACTORY_RUNTIME_PATH";

pub const State = enum {
    stopped,
    starting,
    running,
    failed,
};

const SpinMutex = struct {
    inner: std.atomic.Mutex = .unlocked,

    fn lock(self: *SpinMutex) void {
        while (!self.inner.tryLock()) std.atomic.spinLoopHint();
    }

    fn unlock(self: *SpinMutex) void {
        self.inner.unlock();
    }
};

const Pending = struct {
    in_use: bool = false,
    runtime_id: [max_id_bytes]u8 = undefined,
    runtime_id_len: usize = 0,
    bridge_id: [max_id_bytes]u8 = undefined,
    bridge_id_len: usize = 0,
    responder: native_sdk.bridge.AsyncResponder = undefined,
    response: ?[]u8 = null,
    failure_message: ?[]const u8 = null,

    fn runtimeId(self: *const Pending) []const u8 {
        return self.runtime_id[0..self.runtime_id_len];
    }

    fn bridgeId(self: *const Pending) []const u8 {
        return self.bridge_id[0..self.bridge_id_len];
    }
};

const EventEntry = struct {
    bytes: []u8,
};

const FrameMetadata = struct {
    kind_storage: [max_kind_bytes]u8 = undefined,
    kind_len: usize = 0,
    id_storage: [max_id_bytes]u8 = undefined,
    id_len: usize = 0,

    fn kind(self: *const FrameMetadata) []const u8 {
        return self.kind_storage[0..self.kind_len];
    }

    fn id(self: *const FrameMetadata) ?[]const u8 {
        if (self.id_len == 0) return null;
        return self.id_storage[0..self.id_len];
    }
};

pub const Sidecar = struct {
    io: std.Io,
    env_map: *std.process.Environ.Map,
    allocator: std.mem.Allocator,
    state_mutex: SpinMutex = .{},
    current_state: State = .stopped,
    stopping: std.atomic.Value(bool) = .init(false),
    process_group_id: std.atomic.Value(i32) = .init(0),
    child: std.process.Child = undefined,
    stdin_file: ?std.Io.File = null,
    stdout_thread: ?std.Thread = null,
    stderr_thread: ?std.Thread = null,
    write_mutex: SpinMutex = .{},
    pending_mutex: SpinMutex = .{},
    pending: [max_pending_requests]Pending = @splat(.{}),
    events_mutex: SpinMutex = .{},
    events: std.ArrayList(EventEntry) = .empty,
    cached_event_bytes: usize = 0,
    stderr_mutex: SpinMutex = .{},
    stderr_tail: [max_stderr_bytes]u8 = undefined,
    stderr_len: usize = 0,

    pub fn init(
        io: std.Io,
        env_map: *std.process.Environ.Map,
        allocator: std.mem.Allocator,
    ) Sidecar {
        return .{
            .io = io,
            .env_map = env_map,
            .allocator = allocator,
        };
    }

    pub fn state(self: *Sidecar) State {
        self.state_mutex.lock();
        defer self.state_mutex.unlock();
        return self.current_state;
    }

    fn setState(self: *Sidecar, value: State) void {
        self.state_mutex.lock();
        defer self.state_mutex.unlock();
        self.current_state = value;
    }

    /// Start the single application runtime. Failure is recoverable: the shell
    /// remains open and `runtime.invoke` reports that the runtime is unavailable.
    pub fn start(self: *Sidecar) !void {
        if (self.state() == .running or self.state() == .starting) return;
        self.setState(.starting);
        self.stopping.store(false, .release);

        var packaged_path: [std.Io.Dir.max_path_bytes]u8 = undefined;
        var executable_path: [std.Io.Dir.max_path_bytes]u8 = undefined;
        const resolved_runtime = runtimeLocation(
            self.io,
            &executable_path,
            &packaged_path,
        ) catch null;

        // A packaged host must always execute its sealed runtime. Development
        // overrides and PATH candidates are intentionally ignored in a bundle,
        // so an environment variable cannot replace signed application code.
        if (resolved_runtime) |location| {
            if (location.is_packaged_app) {
                self.spawn(location.path) catch |err| {
                    self.setState(.failed);
                    return err;
                };
                self.setState(.running);
                return;
            }
        }

        const override = self.env_map.get(runtime_path_env);
        if (override) |path| {
            if (path.len > 0) {
                self.spawn(path) catch |err| {
                    self.setState(.failed);
                    return err;
                };
                self.setState(.running);
                return;
            }
        }

        const candidates = [_][]const u8{
            "../../target/debug/agent-factory-runtime",
            bundle_resources.runtime_name,
        };

        var last_error: anyerror = error.RuntimeUnavailable;
        for (candidates) |candidate| {
            self.spawn(candidate) catch |err| {
                last_error = err;
                continue;
            };
            self.setState(.running);
            return;
        }
        self.setState(.failed);
        return last_error;
    }

    fn spawn(self: *Sidecar, executable: []const u8) !void {
        self.child = try std.process.spawn(self.io, .{
            .argv = &.{executable},
            .environ_map = self.env_map,
            .stdin = .pipe,
            .stdout = .pipe,
            .stderr = .pipe,
            // The sidecar owns its own process group, so its descendants can
            // never survive the native host's ordered shutdown.
            .pgid = 0,
        });
        self.process_group_id.store(
            @intCast(self.child.id orelse return error.RuntimeUnavailable),
            .release,
        );
        self.stdin_file = self.child.stdin;
        self.stderr_len = 0;

        self.stderr_thread = std.Thread.spawn(.{}, drainStderr, .{self}) catch |err| {
            self.signalProcessGroup();
            self.forceProcessGroup();
            _ = self.child.wait(self.io) catch {};
            self.process_group_id.store(0, .release);
            self.stdin_file = null;
            return err;
        };
        self.stdout_thread = std.Thread.spawn(.{}, readFrames, .{self}) catch |err| {
            self.signalProcessGroup();
            self.forceProcessGroup();
            if (self.stderr_thread) |thread| thread.join();
            self.stderr_thread = null;
            _ = self.child.wait(self.io) catch {};
            self.process_group_id.store(0, .release);
            self.stdin_file = null;
            return err;
        };
    }

    /// Forward one validated request frame without interpreting its method or
    /// params. Completion occurs on the stdout reader after a matching response.
    pub fn invoke(
        self: *Sidecar,
        frame: []const u8,
        bridge_id: []const u8,
        responder: native_sdk.bridge.AsyncResponder,
    ) !void {
        if (self.state() != .running) return error.RuntimeUnavailable;
        if (frame.len > max_frame_bytes) return error.FrameTooLarge;
        const metadata = try requestMetadata(self.allocator, frame);
        const runtime_id = metadata.id() orelse return error.InvalidRequestFrame;
        const slot = try self.claimPending(runtime_id, bridge_id, responder);
        errdefer self.releasePending(slot);

        const encoded = try self.allocator.alloc(u8, frame.len + 4);
        defer self.allocator.free(encoded);
        writeFrameHeader(encoded[0..4], frame.len);
        @memcpy(encoded[4..], frame);

        self.write_mutex.lock();
        defer self.write_mutex.unlock();
        const stdin_file = self.stdin_file orelse return error.RuntimeUnavailable;
        stdin_file.writeStreamingAll(self.io, encoded) catch |err| {
            self.setState(.failed);
            return err;
        };
    }

    pub fn stop(self: *Sidecar) void {
        if (self.stopping.swap(true, .acq_rel)) return;
        const pid = self.process_group_id.load(.acquire);
        self.signalProcessGroup();
        const watchdog = if (pid > 0)
            std.Thread.spawn(.{}, enforceShutdownDeadline, .{ self, pid }) catch blk: {
                self.forceProcessGroup();
                break :blk null;
            }
        else
            null;
        if (self.stdout_thread) |thread| {
            thread.join();
            self.stdout_thread = null;
        }
        if (self.stderr_thread) |thread| {
            thread.join();
            self.stderr_thread = null;
        }
        if (watchdog) |thread| thread.join();
        self.write_mutex.lock();
        self.stdin_file = null;
        self.write_mutex.unlock();
        self.discardAllPending();
        self.clearEvents();
        self.setState(.stopped);
    }

    /// Complete one response on the application thread. The stdout reader
    /// only fills slots; it never calls Native-SDK platform services.
    pub fn drainCompletion(self: *Sidecar) bool {
        var responder: ?native_sdk.bridge.AsyncResponder = null;
        var bridge_id_storage: [max_id_bytes]u8 = undefined;
        var bridge_id_len: usize = 0;
        var response: ?[]u8 = null;
        var failure_message: ?[]const u8 = null;

        self.pending_mutex.lock();
        for (&self.pending) |*slot| {
            if (!slot.in_use or
                (slot.response == null and slot.failure_message == null))
            {
                continue;
            }
            responder = slot.responder;
            bridge_id_len = slot.bridge_id_len;
            @memcpy(bridge_id_storage[0..bridge_id_len], slot.bridgeId());
            response = slot.response;
            failure_message = slot.failure_message;
            slot.in_use = false;
            slot.runtime_id_len = 0;
            slot.bridge_id_len = 0;
            slot.response = null;
            slot.failure_message = null;
            break;
        }
        self.pending_mutex.unlock();

        const value = responder orelse return false;
        if (response) |bytes| {
            defer self.allocator.free(bytes);
            value.success(bridge_id_storage[0..bridge_id_len], bytes) catch {};
        } else {
            value.fail(
                bridge_id_storage[0..bridge_id_len],
                .internal_error,
                failure_message orelse "Rust runtime transport failed",
            ) catch {};
        }
        return true;
    }

    /// Remove the oldest runtime event into caller-owned storage. The app
    /// thread uses this to emit `runtime:event`; the stdout thread never calls
    /// Native-SDK platform services directly.
    pub fn drainEvent(self: *Sidecar, output: []u8) ?[]const u8 {
        self.events_mutex.lock();
        defer self.events_mutex.unlock();
        if (self.events.items.len == 0) return null;
        const entry = self.events.orderedRemove(0);
        defer self.allocator.free(entry.bytes);
        self.cached_event_bytes -= entry.bytes.len;
        if (entry.bytes.len > output.len) return null;
        @memcpy(output[0..entry.bytes.len], entry.bytes);
        return output[0..entry.bytes.len];
    }

    fn signalProcessGroup(self: *Sidecar) void {
        const pid = self.process_group_id.load(.acquire);
        if (pid > 0) std.posix.kill(-pid, std.posix.SIG.TERM) catch {};
    }

    fn forceProcessGroup(self: *Sidecar) void {
        const pid = self.process_group_id.load(.acquire);
        if (pid > 0) std.posix.kill(-pid, std.posix.SIG.KILL) catch {};
    }

    fn enforceShutdownDeadline(self: *Sidecar, pid: i32) void {
        const polls = shutdown_grace_ms / shutdown_poll_ms;
        for (0..polls) |_| {
            if (self.process_group_id.load(.acquire) != pid) return;
            std.Io.sleep(
                self.io,
                std.Io.Duration.fromMilliseconds(shutdown_poll_ms),
                .awake,
            ) catch break;
        }
        if (self.process_group_id.load(.acquire) == pid) {
            std.posix.kill(-pid, std.posix.SIG.KILL) catch {};
        }
    }

    fn readFrames(self: *Sidecar) void {
        const stdout_file = self.child.stdout orelse {
            self.failRuntime("Rust runtime stdout is unavailable");
            return;
        };
        while (!self.stopping.load(.acquire)) {
            const frame = readFrame(self.allocator, self.io, stdout_file) catch {
                // EOF, malformed UTF-8, and oversized frames all invalidate
                // the transport. Ensure child.wait cannot block on a runtime
                // that kept running after violating the framing contract.
                if (!self.stopping.load(.acquire)) self.signalProcessGroup();
                break;
            };
            defer self.allocator.free(frame);
            self.routeFrame(frame) catch {
                if (!self.stopping.load(.acquire)) self.signalProcessGroup();
                break;
            };
        }
        _ = self.child.wait(self.io) catch {};
        self.process_group_id.store(0, .release);
        self.write_mutex.lock();
        self.stdin_file = null;
        self.write_mutex.unlock();
        if (!self.stopping.load(.acquire)) {
            self.failRuntime("Rust runtime exited unexpectedly");
        }
    }

    fn routeFrame(self: *Sidecar, frame: []const u8) !void {
        const metadata = try parseMetadata(self.allocator, frame);
        if (std.mem.eql(u8, metadata.kind(), "response")) {
            const id = metadata.id() orelse return error.InvalidFrame;
            self.completePending(id, frame);
            return;
        }
        if (std.mem.eql(u8, metadata.kind(), "ready")) {
            // `ready` is transport lifecycle, not a runtime domain event. The
            // browser event channel accepts only sequenced `kind:event`
            // envelopes, so consuming it here prevents a false disconnect.
            return;
        }
        if (std.mem.eql(u8, metadata.kind(), "event")) {
            self.cacheEvent(frame);
            return;
        }
        return error.InvalidFrame;
    }

    fn drainStderr(self: *Sidecar) void {
        const stderr_file = self.child.stderr orelse return;
        var buffer: [4096]u8 = undefined;
        while (!self.stopping.load(.acquire)) {
            const slices: [1][]u8 = .{&buffer};
            const count = stderr_file.readStreaming(self.io, &slices) catch break;
            if (count == 0) break;
            self.appendStderr(buffer[0..count]);
        }
    }

    fn failRuntime(self: *Sidecar, message: []const u8) void {
        self.setState(.failed);
        self.queueAllFailures(message);
    }

    fn claimPending(
        self: *Sidecar,
        runtime_id: []const u8,
        bridge_id: []const u8,
        responder: native_sdk.bridge.AsyncResponder,
    ) !*Pending {
        if (runtime_id.len == 0 or runtime_id.len > max_id_bytes) {
            return error.InvalidRequestFrame;
        }
        if (bridge_id.len == 0 or bridge_id.len > max_id_bytes) {
            return error.InvalidRequestFrame;
        }
        self.pending_mutex.lock();
        defer self.pending_mutex.unlock();
        for (&self.pending) |*slot| {
            if (slot.in_use and std.mem.eql(u8, slot.runtimeId(), runtime_id)) {
                return error.DuplicateRequestId;
            }
        }
        for (&self.pending) |*slot| {
            if (slot.in_use) continue;
            @memcpy(slot.runtime_id[0..runtime_id.len], runtime_id);
            slot.runtime_id_len = runtime_id.len;
            @memcpy(slot.bridge_id[0..bridge_id.len], bridge_id);
            slot.bridge_id_len = bridge_id.len;
            slot.responder = responder;
            slot.response = null;
            slot.failure_message = null;
            slot.in_use = true;
            return slot;
        }
        return error.TooManyPendingRequests;
    }

    fn releasePending(self: *Sidecar, target: *Pending) void {
        self.pending_mutex.lock();
        defer self.pending_mutex.unlock();
        if (target.response) |bytes| self.allocator.free(bytes);
        target.in_use = false;
        target.runtime_id_len = 0;
        target.bridge_id_len = 0;
        target.response = null;
        target.failure_message = null;
    }

    fn completePending(self: *Sidecar, runtime_id: []const u8, frame: []const u8) void {
        self.pending_mutex.lock();
        defer self.pending_mutex.unlock();
        for (&self.pending) |*slot| {
            if (!slot.in_use or !std.mem.eql(u8, slot.runtimeId(), runtime_id)) continue;
            if (slot.response != null or slot.failure_message != null) return;
            slot.response = self.allocator.dupe(u8, frame) catch {
                slot.failure_message = "Rust runtime response could not be buffered";
                return;
            };
            return;
        }
    }

    fn queueAllFailures(self: *Sidecar, message: []const u8) void {
        self.pending_mutex.lock();
        defer self.pending_mutex.unlock();
        for (&self.pending) |*slot| {
            if (!slot.in_use or slot.response != null) continue;
            slot.failure_message = message;
        }
    }

    fn discardAllPending(self: *Sidecar) void {
        self.pending_mutex.lock();
        defer self.pending_mutex.unlock();
        for (&self.pending) |*slot| {
            if (slot.response) |bytes| self.allocator.free(bytes);
            slot.in_use = false;
            slot.runtime_id_len = 0;
            slot.bridge_id_len = 0;
            slot.response = null;
            slot.failure_message = null;
        }
    }

    fn cacheEvent(self: *Sidecar, frame: []const u8) void {
        // Native-SDK window-event details are intentionally small. Oversized
        // events are dropped; their sequence gap causes the UI to request the
        // mandatory `snapshot.get` resynchronization from Rust.
        if (frame.len > native_sdk.platform.max_window_event_detail_bytes) return;
        const copy = self.allocator.dupe(u8, frame) catch return;

        self.events_mutex.lock();
        defer self.events_mutex.unlock();
        while (self.events.items.len >= max_cached_events or
            self.cached_event_bytes + copy.len > max_cached_event_bytes)
        {
            if (self.events.items.len == 0) break;
            const removed = self.events.orderedRemove(0);
            self.cached_event_bytes -= removed.bytes.len;
            self.allocator.free(removed.bytes);
        }
        self.events.append(self.allocator, .{ .bytes = copy }) catch {
            self.allocator.free(copy);
            return;
        };
        self.cached_event_bytes += copy.len;
    }

    fn clearEvents(self: *Sidecar) void {
        self.events_mutex.lock();
        defer self.events_mutex.unlock();
        for (self.events.items) |entry| self.allocator.free(entry.bytes);
        self.events.deinit(self.allocator);
        self.events = .empty;
        self.cached_event_bytes = 0;
    }

    fn appendStderr(self: *Sidecar, bytes: []const u8) void {
        self.stderr_mutex.lock();
        defer self.stderr_mutex.unlock();
        for (bytes) |byte| {
            if (self.stderr_len < self.stderr_tail.len) {
                self.stderr_tail[self.stderr_len] = byte;
                self.stderr_len += 1;
                continue;
            }
            std.mem.copyForwards(
                u8,
                self.stderr_tail[0 .. self.stderr_tail.len - 1],
                self.stderr_tail[1..],
            );
            self.stderr_tail[self.stderr_tail.len - 1] = byte;
        }
    }
};

const RuntimeLocation = struct {
    path: []const u8,
    is_packaged_app: bool,
};

fn runtimeLocation(
    io: std.Io,
    executable_storage: []u8,
    output: []u8,
) !RuntimeLocation {
    const executable_len = try std.process.executablePath(io, executable_storage);
    const executable_path = executable_storage[0..executable_len];
    const path = bundle_resources.sealedPath(
        executable_path,
        bundle_resources.runtime_name,
        output,
    ) catch return error.RuntimeUnavailable;
    return .{
        .path = path,
        .is_packaged_app = true,
    };
}

fn requestMetadata(allocator: std.mem.Allocator, frame: []const u8) !FrameMetadata {
    const metadata = try parseMetadata(allocator, frame);
    if (!std.mem.eql(u8, metadata.kind(), "request") or metadata.id() == null) {
        return error.InvalidRequestFrame;
    }
    return metadata;
}

fn parseMetadata(allocator: std.mem.Allocator, frame: []const u8) !FrameMetadata {
    if (frame.len == 0 or frame.len > max_frame_bytes) return error.InvalidFrame;
    const parsed = std.json.parseFromSlice(
        std.json.Value,
        allocator,
        frame,
        .{},
    ) catch return error.InvalidFrame;
    defer parsed.deinit();
    if (parsed.value != .object) return error.InvalidFrame;
    const kind_value = parsed.value.object.get("kind") orelse return error.InvalidFrame;
    if (kind_value != .string) return error.InvalidFrame;
    if (kind_value.string.len == 0 or kind_value.string.len > max_kind_bytes) {
        return error.InvalidFrame;
    }
    const version_value = parsed.value.object.get("version") orelse return error.InvalidFrame;
    if (version_value != .integer or version_value.integer != protocol_version) {
        return error.InvalidFrame;
    }
    var metadata: FrameMetadata = .{};
    @memcpy(
        metadata.kind_storage[0..kind_value.string.len],
        kind_value.string,
    );
    metadata.kind_len = kind_value.string.len;
    if (parsed.value.object.get("id")) |id_value| {
        if (id_value != .string or
            id_value.string.len == 0 or
            id_value.string.len > max_id_bytes)
        {
            return error.InvalidFrame;
        }
        @memcpy(
            metadata.id_storage[0..id_value.string.len],
            id_value.string,
        );
        metadata.id_len = id_value.string.len;
    }
    return metadata;
}

fn readFrame(
    allocator: std.mem.Allocator,
    io: std.Io,
    file: std.Io.File,
) ![]u8 {
    var header: [4]u8 = undefined;
    try readExactly(io, file, &header);
    const length = readFrameHeader(&header);
    if (length == 0 or length > max_frame_bytes) return error.FrameTooLarge;
    const frame = try allocator.alloc(u8, length);
    errdefer allocator.free(frame);
    try readExactly(io, file, frame);
    if (!std.unicode.utf8ValidateSlice(frame)) return error.InvalidFrame;
    return frame;
}

fn readExactly(io: std.Io, file: std.Io.File, output: []u8) !void {
    var offset: usize = 0;
    while (offset < output.len) {
        const slices: [1][]u8 = .{output[offset..]};
        const count = try file.readStreaming(io, &slices);
        if (count == 0) return error.EndOfStream;
        offset += count;
    }
}

fn writeFrameHeader(output: *[4]u8, length: usize) void {
    const value: u32 = @intCast(length);
    output.* = .{
        @intCast(value >> 24),
        @intCast((value >> 16) & 0xff),
        @intCast((value >> 8) & 0xff),
        @intCast(value & 0xff),
    };
}

fn readFrameHeader(input: *const [4]u8) usize {
    return (@as(usize, input[0]) << 24) |
        (@as(usize, input[1]) << 16) |
        (@as(usize, input[2]) << 8) |
        @as(usize, input[3]);
}

pub fn publicErrorMessage(err: anyerror) []const u8 {
    return switch (err) {
        error.InvalidFrame, error.InvalidRequestFrame => "Runtime request is not a valid protocol v1 request frame",
        error.FrameTooLarge => "Runtime request exceeds the 1 MiB frame limit",
        error.DuplicateRequestId => "Runtime request id is already pending",
        error.TooManyPendingRequests => "Runtime request limit reached",
        error.RuntimeUnavailable => "Rust runtime is unavailable",
        else => "Rust runtime transport failed",
    };
}

test "frame header uses a four-byte big-endian length" {
    var header: [4]u8 = undefined;
    writeFrameHeader(&header, 0x01020304);
    try std.testing.expectEqualSlices(u8, &.{ 1, 2, 3, 4 }, &header);
    try std.testing.expectEqual(@as(usize, 0x01020304), readFrameHeader(&header));
}

test "request metadata validates version kind and id" {
    const metadata = try requestMetadata(
        std.testing.allocator,
        "{\"kind\":\"request\",\"version\":1,\"id\":\"abc\",\"method\":\"snapshot.get\",\"params\":{}}",
    );
    try std.testing.expectEqualStrings("request", metadata.kind());
    try std.testing.expectEqualStrings("abc", metadata.id().?);
    try std.testing.expectError(
        error.InvalidRequestFrame,
        requestMetadata(
            std.testing.allocator,
            "{\"kind\":\"event\",\"version\":1,\"id\":\"abc\"}",
        ),
    );
    try std.testing.expectError(
        error.InvalidFrame,
        requestMetadata(
            std.testing.allocator,
            "{\"kind\":\"request\",\"version\":2,\"id\":\"abc\"}",
        ),
    );
}

test "metadata always uses the top-level request id" {
    const metadata = try requestMetadata(
        std.testing.allocator,
        "{\"kind\":\"request\",\"version\":1,\"params\":{\"id\":\"nested\"},\"id\":\"top-level\",\"method\":\"snapshot.get\"}",
    );
    try std.testing.expectEqualStrings("top-level", metadata.id().?);
}

test "ready is consumed and runtime events drain in arrival order" {
    var env = std.process.Environ.Map.init(std.testing.allocator);
    defer env.deinit();
    var sidecar = Sidecar.init(std.testing.io, &env, std.testing.allocator);
    defer sidecar.clearEvents();
    try sidecar.routeFrame("{\"kind\":\"ready\",\"version\":1}");

    var output: [256]u8 = undefined;
    try std.testing.expect(sidecar.drainEvent(&output) == null);

    try sidecar.routeFrame(
        "{\"kind\":\"event\",\"version\":1,\"sequence\":1}",
    );
    try sidecar.routeFrame(
        "{\"kind\":\"event\",\"version\":1,\"sequence\":2}",
    );
    try std.testing.expectEqualStrings(
        "{\"kind\":\"event\",\"version\":1,\"sequence\":1}",
        sidecar.drainEvent(&output).?,
    );
    try std.testing.expectEqualStrings(
        "{\"kind\":\"event\",\"version\":1,\"sequence\":2}",
        sidecar.drainEvent(&output).?,
    );
    try std.testing.expect(sidecar.drainEvent(&output) == null);
}

test "only application bundle executables are treated as packaged" {
    var storage: [std.Io.Dir.max_path_bytes]u8 = undefined;
    try std.testing.expectEqualStrings(
        "/Applications/Agent Factory.app/Contents/Resources/agent-factory-runtime",
        try bundle_resources.sealedPath(
            "/Applications/Agent Factory.app/Contents/MacOS/agent-factory",
            bundle_resources.runtime_name,
            &storage,
        ),
    );
}

test "public transport errors do not expose operating system details" {
    try std.testing.expectEqualStrings(
        "Rust runtime is unavailable",
        publicErrorMessage(error.RuntimeUnavailable),
    );
    try std.testing.expectEqualStrings(
        "Rust runtime transport failed",
        publicErrorMessage(error.AccessDenied),
    );
}

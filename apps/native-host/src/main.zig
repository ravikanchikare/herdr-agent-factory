const std = @import("std");
const runner = @import("runner");
const native_sdk = @import("native_sdk");
const builtin = @import("builtin");
const bundle_resources = @import("bundle_resources.zig");
const runtime_sidecar = @import("runtime_sidecar.zig");

pub const panic = std.debug.FullPanic(native_sdk.debug.capturePanic);

const canvas = native_sdk.canvas;

const production_origin = "zero://app";
const production_url = "zero://app/index.html";
const development_origin = "http://127.0.0.1:3000";
const allowed_origins = [_][]const u8{ production_origin, development_origin };
const directory_picker_command = "native-sdk.dialog.openFile";
const notification_command = "native-sdk.os.showNotification";
const window_drag_command = "desktop.window.startDrag.v1";
const terminal_show_command = "desktop.terminal.show.v1";
const terminal_hide_command = "desktop.terminal.hide.v1";
const terminal_visibility_event = "desktop:terminal-visibility";
const dialog_permissions = [_][]const u8{native_sdk.security.permission_dialog};
const notification_permissions = [_][]const u8{native_sdk.security.permission_notifications};
const window_permissions = [_][]const u8{native_sdk.security.permission_window};
const terminal_permissions = [_][]const u8{native_sdk.security.permission_command};
const platform_permissions = [_][]const u8{
    native_sdk.security.permission_dialog,
    native_sdk.security.permission_notifications,
    native_sdk.security.permission_window,
    native_sdk.security.permission_view,
    native_sdk.security.permission_command,
};
const bridge_policies = [_]native_sdk.BridgeCommandPolicy{
    .{
        .name = "runtime.invoke",
        .origins = &allowed_origins,
    },
    .{
        .name = window_drag_command,
        .permissions = &window_permissions,
        .origins = &allowed_origins,
    },
    .{
        .name = terminal_show_command,
        .permissions = &terminal_permissions,
        .origins = &allowed_origins,
    },
    .{
        .name = terminal_hide_command,
        .permissions = &terminal_permissions,
        .origins = &allowed_origins,
    },
};
const window_create_command = "native-sdk.window.create";
const window_focus_command = "native-sdk.window.focus";
const window_close_command = "native-sdk.window.close";
const window_list_command = "native-sdk.window.list";
const builtin_commands = [_]native_sdk.BridgeCommandPolicy{
    .{
        .name = directory_picker_command,
        .permissions = &dialog_permissions,
        .origins = &allowed_origins,
    },
    .{
        .name = notification_command,
        .permissions = &notification_permissions,
        .origins = &allowed_origins,
    },
    // Dedicated Draft windows: create/focus/close/list from the web UI.
    .{
        .name = window_create_command,
        .permissions = &window_permissions,
        .origins = &allowed_origins,
    },
    .{
        .name = window_focus_command,
        .permissions = &window_permissions,
        .origins = &allowed_origins,
    },
    .{
        .name = window_close_command,
        .permissions = &window_permissions,
        .origins = &allowed_origins,
    },
    .{
        .name = window_list_command,
        .permissions = &window_permissions,
        .origins = &allowed_origins,
    },
};
const builtin_policy: native_sdk.BridgePolicy = .{
    .enabled = true,
    .permissions = &platform_permissions,
    .commands = &builtin_commands,
};
const runtime_event_timer_id: u64 = 0x4146_5254;
const runtime_event_interval_ns: u64 = 25 * std.time.ns_per_ms;

// The startup window created from app.zon's first shell window receives id 1.
const main_window_id: native_sdk.WindowId = 1;

// Tray (menu-bar status item) command ids.
const open_command = "desktop.open";
const check_updates_command = "desktop.check-updates";
const quit_command = "desktop.quit";
const check_updates_event = "desktop:check-updates";

const tray_items = [_]native_sdk.TrayMenuItem{
    .{ .id = 1, .label = "Open Agent Factory", .command = open_command },
    .{ .separator = true },
    .{ .id = 2, .label = "Check for Updates…", .command = check_updates_command },
    .{ .separator = true },
    .{ .id = 3, .label = "Quit Agent Factory", .command = quit_command },
};

const canvas_label = "agent-factory-canvas";
const web_view_label = "agent-factory-web";
const web_pane_anchor = "web-pane";
const terminal_effect_key: u64 = 1;
const terminal_label_capacity = 256;
const terminal_initial_cols: u16 = 120;
const terminal_initial_rows: u16 = 40;

pub const Model = struct {
    web_url: []const u8 = production_url,
    // The first pane is the WebView. Thirty percent leaves the revealed
    // terminal exactly seventy percent of the application width.
    split_fraction: f32 = 0.3,
    terminal_visible: bool = false,
    terminal_started: bool = false,
    terminal_live: bool = false,
    terminal_scrollback: u32 = 0,
    terminal_label_buffer: canvas.TextBuffer(terminal_label_capacity) = .{},

    pub fn terminal_key(model: *const Model) u64 {
        _ = model;
        return terminal_effect_key;
    }

    pub fn terminal_label(model: *const Model) []const u8 {
        const label = model.terminal_label_buffer.text();
        return if (label.len > 0) label else "Herdr workspace";
    }
};

const TerminalLaunch = struct {
    executable: []const u8,
    arguments: []const []const u8,
    label: []const u8,
};

pub const Msg = union(enum) {
    show_terminal: TerminalLaunch,
    hide_terminal,
    terminal_event: native_sdk.EffectPtyEvent,
    terminal_state: canvas.TerminalState,
    split_resized: f32,
};

const ShellUiApp = native_sdk.UiAppWithFeatures(
    Model,
    Msg,
    .{ .runtime_markup = builtin.mode == .Debug },
);
const Effects = ShellUiApp.Effects;

pub fn update(model: *Model, msg: Msg, fx: *Effects) void {
    switch (msg) {
        .show_terminal => |launch| {
            model.terminal_visible = true;
            model.terminal_label_buffer.set(launch.label);
            if (model.terminal_started or launch.arguments.len > 2) return;

            var argv: [3][]const u8 = undefined;
            argv[0] = launch.executable;
            for (launch.arguments, 1..) |argument, index| {
                argv[index] = argument;
            }
            model.terminal_started = true;
            fx.ptySpawn(.{
                .key = terminal_effect_key,
                .argv = argv[0 .. launch.arguments.len + 1],
                .cols = terminal_initial_cols,
                .rows = terminal_initial_rows,
                .on_event = Effects.ptyMsg(.terminal_event),
            });
        },
        .hide_terminal => model.terminal_visible = false,
        .terminal_event => |event| switch (event.kind) {
            .output => {
                model.terminal_live = true;
            },
            .exit => {
                model.terminal_visible = false;
                model.terminal_started = false;
                model.terminal_live = false;
            },
            .write => unreachable,
        },
        .terminal_state => |state| model.terminal_scrollback = state.scrollback,
        .split_resized => |fraction| model.split_fraction = fraction,
    }
}

const shell_markup = @embedFile("agent_factory.native");
const CompiledShellView = canvas.CompiledMarkupView(Model, Msg, shell_markup);

fn webPanes(model: *const Model, out: []ShellUiApp.WebViewPane) usize {
    out[0] = .{
        .label = web_view_label,
        .anchor = web_pane_anchor,
        .url = model.web_url,
    };
    return 1;
}

const shell_views = [_]native_sdk.ShellView{
    .{
        .label = canvas_label,
        .kind = .gpu_surface,
        .fill = true,
        .role = "Agent Factory shell",
        .accessibility_label = "Agent Factory",
        .gpu_backend = .metal,
        .gpu_pixel_format = .bgra8_unorm,
        .gpu_present_mode = .timer,
        .gpu_alpha_mode = .@"opaque",
        .gpu_color_space = .srgb,
        .gpu_vsync = true,
    },
    .{
        .label = web_view_label,
        .kind = .webview,
        .parent = canvas_label,
        .url = production_url,
        .x = 0,
        .y = 0,
        .width = 1,
        .height = 1,
        .layer = 20,
    },
};
const shell_windows = [_]native_sdk.ShellWindow{.{
    .label = "main",
    .title = "Agent Factory",
    .width = 1440,
    .height = 960,
    .min_width = 960,
    .min_height = 640,
    .restore_state = true,
    .titlebar = .hidden_inset,
    .close_policy = .hide,
    .views = &shell_views,
}};
const shell_scene: native_sdk.ShellConfig = .{ .windows = &shell_windows };

fn shellAppOptions(io: std.Io) ShellUiApp.Options {
    return .{
        .name = "agent-factory-shell",
        .scene = shell_scene,
        .canvas_label = canvas_label,
        .update_fx = update,
        .view = CompiledShellView.build,
        .markup = if (builtin.mode == .Debug)
            .{ .source = shell_markup, .watch_path = "src/agent_factory.native", .io = io }
        else
            null,
        .web_panes = webPanes,
    };
}

const App = struct {
    env_map: *std.process.Environ.Map,
    ui: *ShellUiApp,
    sidecar: runtime_sidecar.Sidecar,
    platform_bridge: PlatformBridge,
    runtime_bridge_handlers: [1]native_sdk.bridge.AsyncHandler = undefined,

    fn init(init_value: std.process.Init, ui: *ShellUiApp) App {
        return .{
            .env_map = init_value.environ_map,
            .ui = ui,
            .sidecar = runtime_sidecar.Sidecar.init(
                init_value.io,
                init_value.environ_map,
                std.heap.page_allocator,
            ),
            .platform_bridge = .{ .ui = ui },
        };
    }

    fn app(self: *@This()) native_sdk.App {
        return .{
            .context = self,
            .name = "agent-factory",
            // Dynamically-created Draft WebViews still use the packaged or dev
            // frontend source while the main WebView is scene-managed.
            .source = native_sdk.frontend.productionSource(.{
                .dist = "frontend/out",
                .entry = "index.html",
                .origin = production_origin,
            }),
            .source_fn = source,
            .scene_fn = scene,
            .start_fn = start,
            .event_fn = event,
            .stop_fn = stop,
            .replay_fn = replay,
        };
    }

    fn source(context: *anyopaque) anyerror!native_sdk.WebViewSource {
        const self: *@This() = @ptrCast(@alignCast(context));
        return native_sdk.frontend.sourceFromEnv(self.env_map, .{
            .dist = "frontend/out",
            .entry = "index.html",
            .origin = production_origin,
        });
    }

    fn scene(context: *anyopaque) anyerror!native_sdk.ShellConfig {
        const self: *@This() = @ptrCast(@alignCast(context));
        return (try self.ui.app().scene()) orelse error.MissingShellScene;
    }

    fn start(context: *anyopaque, runtime: *native_sdk.Runtime) anyerror!void {
        const self: *@This() = @ptrCast(@alignCast(context));
        self.platform_bridge.platform_services = runtime.options.platform.services;
        self.platform_bridge.runtime = runtime;
        self.sidecar.start() catch |err| {
            // A missing runtime is a recoverable degraded state. Every bridge
            // call receives a typed transport failure until next launch.
            std.debug.print("runtime sidecar unavailable: {s}\n", .{@errorName(err)});
        };
        try runtime.startTimer(
            runtime_event_timer_id,
            runtime_event_interval_ns,
            true,
        );
        runtime.createTray(.{
            .icon_path = "assets/icon.png",
            .tooltip = "Agent Factory",
            .items = &tray_items,
        }) catch |err| {
            std.debug.print("menu bar status item unavailable: {s}\n", .{@errorName(err)});
        };
    }

    fn event(
        context: *anyopaque,
        runtime: *native_sdk.Runtime,
        event_value: native_sdk.Event,
    ) anyerror!void {
        const self: *@This() = @ptrCast(@alignCast(context));
        try self.ui.app().event(runtime, event_value);
        self.platform_bridge.publishTerminalVisibility(
            runtime,
            self.ui.model.terminal_visible,
        );
        switch (event_value) {
            .timer => |timer| {
                if (timer.id != runtime_event_timer_id) return;
                // Native-SDK work stays on the application thread. Bounded
                // batches avoid starving window input during event bursts.
                for (0..16) |_| {
                    if (!self.sidecar.drainCompletion()) break;
                }
                var detail_storage: [native_sdk.platform.max_window_event_detail_bytes]u8 = undefined;
                for (0..16) |_| {
                    const detail = self.sidecar.drainEvent(&detail_storage) orelse break;
                    emitRuntimeEventToOpenWindows(runtime, detail);
                }
            },
            .command => |command| try handleCommand(runtime, command),
            else => {},
        }
    }

    fn stop(context: *anyopaque, runtime: *native_sdk.Runtime) anyerror!void {
        const self: *@This() = @ptrCast(@alignCast(context));
        self.sidecar.stop();
        try self.ui.app().stop(runtime);
        self.platform_bridge.platform_services = null;
        self.platform_bridge.runtime = null;
    }

    fn replay(
        context: *anyopaque,
        control: native_sdk.runtime.ReplayControl,
    ) anyerror!void {
        const self: *@This() = @ptrCast(@alignCast(context));
        try self.ui.app().replayControl(control);
    }

    fn bridge(self: *@This()) native_sdk.BridgeDispatcher {
        self.runtime_bridge_handlers = .{.{
            .name = "runtime.invoke",
            .context = self,
            .invoke_fn = invokeRuntime,
        }};
        return .{
            .policy = .{
                .enabled = true,
                .permissions = &platform_permissions,
                .commands = &bridge_policies,
            },
            .registry = self.platform_bridge.registry(),
            .async_registry = .{ .handlers = &self.runtime_bridge_handlers },
        };
    }

    fn invokeRuntime(
        context: *anyopaque,
        invocation: native_sdk.bridge.Invocation,
        responder: native_sdk.bridge.AsyncResponder,
    ) anyerror!void {
        const self: *@This() = @ptrCast(@alignCast(context));
        self.sidecar.invoke(
            invocation.request.payload,
            invocation.request.id,
            responder,
        ) catch |err| {
            const code: native_sdk.bridge.ErrorCode = switch (err) {
                error.InvalidFrame, error.InvalidRequestFrame => .invalid_request,
                error.FrameTooLarge => .payload_too_large,
                else => .internal_error,
            };
            try responder.fail(
                invocation.request.id,
                code,
                runtime_sidecar.publicErrorMessage(err),
            );
        };
    }
};

/// Every WebView owns its own runtime client. Deliver invalidations to each
/// open native window so a dedicated Draft window cannot remain pinned to its
/// bootstrap snapshot while the main window continues to update.
fn emitRuntimeEventToOpenWindows(
    runtime: *native_sdk.Runtime,
    detail: []const u8,
) void {
    var windows: [native_sdk.platform.max_windows]native_sdk.platform.WindowInfo = undefined;
    for (runtime.listWindows(&windows)) |window| {
        if (!window.open) continue;
        runtime.emitWindowEvent(window.id, "runtime:event", detail) catch |err| {
            std.debug.print(
                "runtime event delivery failed for window {d}: {s}\n",
                .{ window.id, @errorName(err) },
            );
        };
    }
}

const TerminalShowPayload = struct {
    executable: []const u8,
    arguments: []const []const u8,
    workspaceId: []const u8,
    label: []const u8,
};

const PlatformBridge = struct {
    platform_services: ?native_sdk.platform.PlatformServices = null,
    runtime: ?*native_sdk.Runtime = null,
    ui: ?*ShellUiApp = null,
    terminal_visibility_published: bool = false,
    handlers: [3]native_sdk.bridge.Handler = undefined,

    fn registry(self: *@This()) native_sdk.bridge.Registry {
        self.handlers = .{
            .{
                .name = window_drag_command,
                .context = self,
                .invoke_fn = startWindowDrag,
            },
            .{
                .name = terminal_show_command,
                .context = self,
                .invoke_fn = showTerminal,
            },
            .{
                .name = terminal_hide_command,
                .context = self,
                .invoke_fn = hideTerminal,
            },
        };
        return .{ .handlers = &self.handlers };
    }

    fn startWindowDrag(
        context: *anyopaque,
        invocation: native_sdk.bridge.Invocation,
        output: []u8,
    ) anyerror![]const u8 {
        const self: *@This() = @ptrCast(@alignCast(context));
        try requireMacosWindowDrag(builtin.os.tag);
        if (!isNullJson(invocation.request.payload)) return error.InvalidPayload;
        const services = self.platform_services orelse return error.PlatformServicesUnavailable;
        try services.startWindowDrag(invocation.source.window_id);
        return std.fmt.bufPrint(
            output,
            "{{\"version\":1,\"windowId\":{d}}}",
            .{invocation.source.window_id},
        );
    }

    fn showTerminal(
        context: *anyopaque,
        invocation: native_sdk.bridge.Invocation,
        output: []u8,
    ) anyerror![]const u8 {
        const self: *@This() = @ptrCast(@alignCast(context));
        const runtime = self.runtime orelse return error.RuntimeUnavailable;
        const ui = self.ui orelse return error.NativeShellUnavailable;
        const parsed = std.json.parseFromSlice(
            TerminalShowPayload,
            std.heap.page_allocator,
            invocation.request.payload,
            .{},
        ) catch return error.InvalidPayload;
        defer parsed.deinit();
        try validateTerminalShowPayload(parsed.value);

        try ui.dispatch(runtime, main_window_id, .{ .show_terminal = .{
            .executable = parsed.value.executable,
            .arguments = parsed.value.arguments,
            .label = parsed.value.label,
        } });
        try runtime.showWindow(main_window_id);
        self.publishTerminalVisibility(runtime, true);
        return std.fmt.bufPrint(
            output,
            "{{\"version\":1,\"visible\":true}}",
            .{},
        );
    }

    fn hideTerminal(
        context: *anyopaque,
        invocation: native_sdk.bridge.Invocation,
        output: []u8,
    ) anyerror![]const u8 {
        const self: *@This() = @ptrCast(@alignCast(context));
        const runtime = self.runtime orelse return error.RuntimeUnavailable;
        const ui = self.ui orelse return error.NativeShellUnavailable;
        if (!isNullJson(invocation.request.payload)) return error.InvalidPayload;

        try ui.dispatch(runtime, main_window_id, .hide_terminal);
        self.publishTerminalVisibility(runtime, false);
        return std.fmt.bufPrint(
            output,
            "{{\"version\":1,\"visible\":false}}",
            .{},
        );
    }

    fn publishTerminalVisibility(
        self: *@This(),
        runtime: *native_sdk.Runtime,
        visible: bool,
    ) void {
        if (self.terminal_visibility_published == visible) return;
        runtime.emitWindowEvent(
            main_window_id,
            terminal_visibility_event,
            if (visible)
                "{\"version\":1,\"visible\":true}"
            else
                "{\"version\":1,\"visible\":false}",
        ) catch |err| {
            std.debug.print(
                "terminal visibility event unavailable: {s}\n",
                .{@errorName(err)},
            );
        };
        self.terminal_visibility_published = visible;
    }
};

fn platformDispatcher(bridge: *PlatformBridge) native_sdk.BridgeDispatcher {
    return .{
        .policy = .{
            .enabled = true,
            .permissions = &platform_permissions,
            .commands = &bridge_policies,
        },
        .registry = bridge.registry(),
    };
}

fn validateTerminalShowPayload(payload: TerminalShowPayload) !void {
    if (payload.executable.len == 0 or payload.executable.len > 4096) {
        return error.InvalidTerminalExecutable;
    }
    if (!std.fs.path.isAbsolute(payload.executable) or
        !std.mem.eql(u8, std.fs.path.basename(payload.executable), "herdr") or
        containsNul(payload.executable))
    {
        return error.InvalidTerminalExecutable;
    }
    if (payload.arguments.len != 0 and
        (payload.arguments.len != 2 or
            !std.mem.eql(u8, payload.arguments[0], "--session") or
            payload.arguments[1].len == 0 or
            payload.arguments[1].len > 128 or
            containsControl(payload.arguments[1])))
    {
        return error.InvalidTerminalArguments;
    }
    if (payload.workspaceId.len == 0 or
        payload.workspaceId.len > 128 or
        containsControl(payload.workspaceId))
    {
        return error.InvalidWorkspaceIdentity;
    }
    if (payload.label.len == 0 or
        payload.label.len > terminal_label_capacity or
        containsNul(payload.label))
    {
        return error.InvalidTerminalLabel;
    }
}

fn containsNul(value: []const u8) bool {
    return std.mem.indexOfScalar(u8, value, 0) != null;
}

fn containsControl(value: []const u8) bool {
    for (value) |byte| {
        if (byte < 0x20 or byte == 0x7f) return true;
    }
    return false;
}

fn isNullJson(payload: []const u8) bool {
    return std.mem.eql(u8, std.mem.trim(u8, payload, " \t\r\n"), "null");
}

fn requireMacosWindowDrag(comptime os_tag: std.Target.Os.Tag) !void {
    if (os_tag != .macos) return error.UnsupportedPlatform;
}

fn handleCommand(
    runtime: *native_sdk.Runtime,
    command: native_sdk.CommandEvent,
) !void {
    if (std.mem.eql(u8, command.name, open_command)) {
        try runtime.showWindow(main_window_id);
        return;
    }
    if (std.mem.eql(u8, command.name, quit_command)) {
        try runtime.quitApp();
        return;
    }
    if (std.mem.eql(u8, command.name, check_updates_command)) {
        try runtime.emitWindowEvent(main_window_id, check_updates_event, "{}");
        return;
    }
}

pub fn main(init_value: std.process.Init) !void {
    var executable_storage: [std.Io.Dir.max_path_bytes]u8 = undefined;
    var config_storage: [std.Io.Dir.max_path_bytes]u8 = undefined;
    const herdr_config_path = if (std.process.executablePath(
        init_value.io,
        &executable_storage,
    )) |length| config: {
        const executable = executable_storage[0..length];
        break :config bundle_resources.sealedPath(
            executable,
            bundle_resources.herdr_config_name,
            &config_storage,
        ) catch sibling_config: {
            const executable_dir = std.fs.path.dirname(executable) orelse ".";
            break :sibling_config std.fmt.bufPrint(
                &config_storage,
                "{s}/{s}",
                .{ executable_dir, bundle_resources.herdr_config_name },
            ) catch bundle_resources.herdr_config_name;
        };
    } else |_| bundle_resources.herdr_config_name;
    try init_value.environ_map.put("HERDR_CONFIG_PATH", herdr_config_path);

    const ui = try ShellUiApp.create(
        std.heap.page_allocator,
        shellAppOptions(init_value.io),
    );
    defer ui.destroy();
    ui.model.web_url = init_value.environ_map.get("NATIVE_SDK_FRONTEND_URL") orelse
        production_url;

    var app = App.init(init_value, ui);
    try runner.runWithOptions(app.app(), .{
        .app_name = "Agent Factory",
        .window_title = "Agent Factory",
        .bundle_id = "app.agentfactory.desktop",
        .icon_path = "assets/icon.png",
        .bridge = app.bridge(),
        .builtin_bridge = builtin_policy,
        .js_window_api = false,
        .security = .{
            .permissions = &platform_permissions,
            .navigation = .{ .allowed_origins = &allowed_origins },
        },
    }, init_value);
}

test "production source is the packaged Next static export" {
    _ = bundle_resources;
    const source_value = native_sdk.frontend.productionSource(.{
        .dist = "frontend/out",
        .entry = "index.html",
        .origin = production_origin,
    });
    try std.testing.expectEqual(
        native_sdk.WebViewSourceKind.assets,
        source_value.kind,
    );
    try std.testing.expectEqualStrings(
        "frontend/out",
        source_value.asset_options.?.root_path,
    );
    try std.testing.expectEqualStrings(
        production_origin,
        source_value.asset_options.?.origin,
    );
}

test "native shell starts with a hidden terminal and a scene WebView" {
    const model: Model = .{};
    try std.testing.expect(!model.terminal_visible);
    try std.testing.expectApproxEqAbs(@as(f32, 0.3), model.split_fraction, 0.001);
    try std.testing.expectEqual(@as(usize, 2), shell_views.len);
    try std.testing.expect(shell_views[0].kind == .gpu_surface);
    try std.testing.expect(shell_views[1].kind == .webview);
    try std.testing.expect(std.mem.indexOf(u8, shell_markup, "<terminal") != null);
}

fn buildShellTree(
    allocator: std.mem.Allocator,
    model: *const Model,
) !canvas.Ui(Msg).Tree {
    var ui = canvas.Ui(Msg).init(allocator);
    return ui.finalize(CompiledShellView.build(&ui, model));
}

fn countWidgetKind(widget: canvas.Widget, kind: canvas.WidgetKind) usize {
    var count: usize = if (widget.kind == kind) 1 else 0;
    for (widget.children) |child| count += countWidgetKind(child, kind);
    return count;
}

fn findWidgetKind(widget: canvas.Widget, kind: canvas.WidgetKind) ?canvas.Widget {
    if (widget.kind == kind) return widget;
    for (widget.children) |child| {
        if (findWidgetKind(child, kind)) |found| return found;
    }
    return null;
}

fn findWidgetLabel(widget: canvas.Widget, label: []const u8) ?canvas.Widget {
    if (std.mem.eql(u8, widget.semantics.label, label)) return widget;
    for (widget.children) |child| {
        if (findWidgetLabel(child, label)) |found| return found;
    }
    return null;
}

test "Open reveals a seventy-percent terminal and spawns one Herdr TUI" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var model: Model = .{};

    const hidden = try buildShellTree(arena.allocator(), &model);
    try std.testing.expectEqual(@as(usize, 0), countWidgetKind(hidden.root, .split));
    try std.testing.expectEqual(@as(usize, 0), countWidgetKind(hidden.root, .terminal));
    try std.testing.expect(findWidgetLabel(hidden.root, web_pane_anchor) != null);

    var fx = Effects.init(std.testing.allocator);
    defer fx.deinit();
    fx.executor = .fake;
    update(&model, .{ .show_terminal = .{
        .executable = "/opt/homebrew/bin/herdr",
        .arguments = &.{ "--session", "agent-factory-dev" },
        .label = "Weather Reporter / main",
    } }, &fx);

    try std.testing.expect(model.terminal_visible);
    try std.testing.expectEqual(@as(usize, 1), fx.pendingPtyCount());
    const request = fx.pendingPtyAt(0) orelse return error.ExpectedHerdrPty;
    try std.testing.expectEqualStrings("/opt/homebrew/bin/herdr", request.argv[0]);
    try std.testing.expectEqualStrings("--session", request.argv[1]);
    try std.testing.expectEqualStrings("agent-factory-dev", request.argv[2]);

    const revealed = try buildShellTree(arena.allocator(), &model);
    try std.testing.expectEqual(@as(usize, 1), countWidgetKind(revealed.root, .split));
    try std.testing.expectEqual(@as(usize, 1), countWidgetKind(revealed.root, .terminal));
    const split = findWidgetKind(revealed.root, .split) orelse
        return error.ExpectedNativeSplit;
    try std.testing.expectApproxEqAbs(@as(f32, 0.3), split.value, 0.001);
    const terminal = findWidgetLabel(
        revealed.root,
        "Weather Reporter / main",
    ) orelse return error.ExpectedNativeTerminal;
    try std.testing.expect(terminal.kind == .terminal);
    try std.testing.expect(findWidgetLabel(revealed.root, "Herdr terminal toolbar") == null);
    try std.testing.expect(findWidgetLabel(revealed.root, "Close Herdr terminal") == null);

    update(&model, .hide_terminal, &fx);
    const hidden_again = try buildShellTree(arena.allocator(), &model);
    try std.testing.expectEqual(
        @as(usize, 0),
        countWidgetKind(hidden_again.root, .terminal),
    );
    try std.testing.expectEqual(@as(usize, 1), fx.pendingPtyCount());
}

test "terminal PTY follows split and window dimensions" {
    if (comptime !native_sdk.runtime.terminal_sessions_enabled) {
        return error.SkipZigTest;
    }

    const harness = try native_sdk.TestHarness().create(std.testing.allocator, .{
        .size = native_sdk.geometry.SizeF.init(1440, 960),
    });
    defer harness.destroy(std.testing.allocator);
    harness.null_platform.gpu_surfaces = true;
    harness.runtime.options.security.navigation.allowed_origins = &allowed_origins;

    const ui = try ShellUiApp.create(std.testing.allocator, .{
        .name = "agent-factory-shell-test",
        .scene = shell_scene,
        .canvas_label = canvas_label,
        .update_fx = update,
        .view = CompiledShellView.build,
        .web_panes = webPanes,
    });
    defer ui.destroy();

    const app = ui.app();
    try harness.start(app);
    ui.effects.executor = .fake;
    try harness.runtime.dispatchPlatformEvent(app, .{ .gpu_surface_frame = .{
        .label = canvas_label,
        .size = native_sdk.geometry.SizeF.init(1440, 960),
        .scale_factor = 2,
        .frame_index = 1,
        .timestamp_ns = 16_000_000,
    } });

    try ui.dispatch(&harness.runtime, main_window_id, .{ .show_terminal = .{
        .executable = "/opt/homebrew/bin/herdr",
        .arguments = &.{ "--session", "agent-factory-dev" },
        .label = "Weather Reporter / main",
    } });
    const initial = ui.effects.ptySize(terminal_effect_key) orelse
        return error.ExpectedInitialTerminalSize;
    try std.testing.expect(initial.cols > 0 and initial.rows > 0);
    try std.testing.expect(
        initial.cols != terminal_initial_cols or
            initial.rows != terminal_initial_rows,
    );

    try ui.dispatch(
        &harness.runtime,
        main_window_id,
        .{ .split_resized = 0.45 },
    );
    const split_resized = ui.effects.ptySize(terminal_effect_key) orelse
        return error.ExpectedSplitTerminalSize;
    try std.testing.expect(split_resized.cols < initial.cols);
    try std.testing.expectEqual(initial.rows, split_resized.rows);

    try harness.runtime.dispatchPlatformEvent(app, .{ .gpu_surface_frame = .{
        .label = canvas_label,
        .size = native_sdk.geometry.SizeF.init(1200, 720),
        .scale_factor = 2,
        .frame_index = 2,
        .timestamp_ns = 32_000_000,
    } });
    const window_resized = ui.effects.ptySize(terminal_effect_key) orelse
        return error.ExpectedWindowTerminalSize;
    try std.testing.expect(window_resized.cols < split_resized.cols);
    try std.testing.expect(window_resized.rows < split_resized.rows);
}

test "runtime and terminal bridges deny untrusted origins" {
    var app: App = undefined;
    app.platform_bridge = .{};
    const dispatcher = app.bridge();

    for ([_][]const u8{
        "runtime.invoke",
        window_drag_command,
        terminal_show_command,
        terminal_hide_command,
    }) |command| {
        try std.testing.expect(dispatcher.policy.allows(command, production_origin));
        try std.testing.expect(dispatcher.policy.allows(command, development_origin));
        try std.testing.expect(!dispatcher.policy.allows(command, "zero://inline"));
        try std.testing.expect(!dispatcher.policy.allows(command, "https://example.com"));
    }
    try std.testing.expect(dispatcher.policy.find("native-sdk.window.create") == null);
}

test "window drag bridge uses the invocation source window synchronously" {
    var bridge: PlatformBridge = .{};
    var null_platform = native_sdk.NullPlatform{};
    const platform = null_platform.platform();
    _ = try platform.services.createWindow(.{
        .id = 2,
        .label = "secondary",
        .title = "Secondary",
    });
    bridge.platform_services = platform.services;

    var response_buffer: [512]u8 = undefined;
    const response = platformDispatcher(&bridge).dispatch(
        "{\"id\":\"drag-1\",\"command\":\"desktop.window.startDrag.v1\",\"payload\":null}",
        .{
            .origin = production_origin,
            .window_id = 2,
            .webview_label = "secondary",
        },
        &response_buffer,
    );

    try std.testing.expectEqual(@as(usize, 1), null_platform.window_drag_start_count);
    try std.testing.expectEqual(@as(native_sdk.WindowId, 2), null_platform.window_drag_starts[0]);
    try std.testing.expect(std.mem.indexOf(u8, response, "\"version\":1") != null);
    try std.testing.expect(std.mem.indexOf(u8, response, "\"windowId\":2") != null);
}

test "runtime events reach every open Agent Factory window" {
    const TestApp = struct {
        fn app(self: *@This()) native_sdk.App {
            return .{
                .context = self,
                .name = "runtime-event-fanout",
                .source = native_sdk.WebViewSource.html("<p>Agent Factory</p>"),
            };
        }
    };

    const harness = try native_sdk.TestHarness().create(
        std.testing.allocator,
        .{},
    );
    defer harness.destroy(std.testing.allocator);
    var app_state: TestApp = .{};
    try harness.start(app_state.app());
    _ = try harness.runtime.createWindow(.{
        .label = "draft-1",
        .title = "Draft",
    });
    const events_before = harness.null_platform.windowEventCount();

    emitRuntimeEventToOpenWindows(
        &harness.runtime,
        "{\"kind\":\"event\"}",
    );

    try std.testing.expectEqual(
        events_before + 2,
        harness.null_platform.windowEventCount(),
    );
    try std.testing.expectEqual(
        @as(native_sdk.WindowId, 2),
        harness.null_platform.lastWindowEventWindowId(),
    );
    try std.testing.expectEqualStrings(
        "runtime:event",
        harness.null_platform.lastWindowEventName(),
    );
}

test "window drag bridge requires null payload and platform services" {
    var bridge: PlatformBridge = .{};
    var response_buffer: [512]u8 = undefined;
    const missing_services = platformDispatcher(&bridge).dispatch(
        "{\"id\":\"drag-1\",\"command\":\"desktop.window.startDrag.v1\",\"payload\":null}",
        .{ .origin = production_origin },
        &response_buffer,
    );
    try std.testing.expect(std.mem.indexOf(u8, missing_services, "PlatformServicesUnavailable") != null);

    var null_platform = native_sdk.NullPlatform{};
    const platform = null_platform.platform();
    bridge.platform_services = platform.services;
    const invalid_payload = platformDispatcher(&bridge).dispatch(
        "{\"id\":\"drag-2\",\"command\":\"desktop.window.startDrag.v1\",\"payload\":{}}",
        .{ .origin = production_origin },
        &response_buffer,
    );
    try std.testing.expect(std.mem.indexOf(u8, invalid_payload, "InvalidPayload") != null);
}

test "native terminal bridge accepts only a Herdr client launch" {
    try validateTerminalShowPayload(.{
        .executable = "/opt/homebrew/bin/herdr",
        .arguments = &.{ "--session", "agent-factory-dev" },
        .workspaceId = "w1",
        .label = "Weather Reporter / main",
    });
    try std.testing.expectError(
        error.InvalidTerminalExecutable,
        validateTerminalShowPayload(.{
            .executable = "/bin/zsh",
            .arguments = &.{},
            .workspaceId = "w1",
            .label = "Workspace",
        }),
    );
    try std.testing.expectError(
        error.InvalidTerminalArguments,
        validateTerminalShowPayload(.{
            .executable = "/opt/homebrew/bin/herdr",
            .arguments = &.{ "terminal", "session" },
            .workspaceId = "w1",
            .label = "Workspace",
        }),
    );
}

test "window drag is macOS-only and runtime invoke stays asynchronous" {
    try std.testing.expectError(error.UnsupportedPlatform, requireMacosWindowDrag(.linux));
    try requireMacosWindowDrag(.macos);

    var app: App = undefined;
    app.platform_bridge = .{};
    const dispatcher = app.bridge();
    try std.testing.expect(dispatcher.registry.find(window_drag_command) != null);
    try std.testing.expect(dispatcher.registry.find(terminal_show_command) != null);
    try std.testing.expect(dispatcher.registry.find(terminal_hide_command) != null);
    try std.testing.expect(dispatcher.registry.find("runtime.invoke") == null);
    try std.testing.expect(dispatcher.async_registry.find("runtime.invoke") != null);
    try std.testing.expect(dispatcher.async_registry.find(window_drag_command) == null);

    const terminal_policy = dispatcher.policy.find(terminal_show_command).?;
    try std.testing.expectEqual(@as(usize, 1), terminal_policy.permissions.len);
    try std.testing.expectEqualStrings(
        native_sdk.security.permission_command,
        terminal_policy.permissions[0],
    );
}

test "only directory picker notifications and Draft windows are builtin" {
    for ([_][]const u8{
        directory_picker_command,
        notification_command,
        window_create_command,
        window_focus_command,
        window_close_command,
        window_list_command,
    }) |command| {
        try std.testing.expect(builtin_policy.allows(command, production_origin));
        try std.testing.expect(!builtin_policy.allows(command, "zero://inline"));
    }

    for ([_][]const u8{
        "native-sdk.dialog.saveFile",
        "native-sdk.dialog.showMessage",
        "native-sdk.os.openUrl",
        "native-sdk.os.revealPath",
        "native-sdk.platform.supports",
    }) |command| {
        try std.testing.expect(!builtin_policy.allows(command, production_origin));
    }
}

test "builtin commands retain least-privilege permissions" {
    const directory = builtin_policy.find(directory_picker_command).?;
    try std.testing.expectEqual(@as(usize, 1), directory.permissions.len);
    try std.testing.expectEqualStrings(
        native_sdk.security.permission_dialog,
        directory.permissions[0],
    );

    const notification = builtin_policy.find(notification_command).?;
    try std.testing.expectEqual(@as(usize, 1), notification.permissions.len);
    try std.testing.expectEqualStrings(
        native_sdk.security.permission_notifications,
        notification.permissions[0],
    );

    const window_create = builtin_policy.find(window_create_command).?;
    try std.testing.expectEqual(@as(usize, 1), window_create.permissions.len);
    try std.testing.expectEqualStrings(
        native_sdk.security.permission_window,
        window_create.permissions[0],
    );
}

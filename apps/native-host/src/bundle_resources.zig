//! Exact resource resolution for code sealed inside the application bundle.
//!
//! Packaged code must never use PATH or an environment override to locate an
//! executable that participates in the update trust boundary.

const std = @import("std");

pub const runtime_name = "agent-factory-runtime";
pub const updater_name = "updater-helper";
pub const update_config_name = "update-config-v1.json";
pub const herdr_config_name = "herdr-client.toml";
const host_name = "agent-factory";
const executable_marker = "/Contents/MacOS/";

pub fn sealedPath(
    executable_path: []const u8,
    resource_name: []const u8,
    output: []u8,
) ![]const u8 {
    if (!std.mem.eql(u8, resource_name, runtime_name) and
        !std.mem.eql(u8, resource_name, updater_name) and
        !std.mem.eql(u8, resource_name, update_config_name) and
        !std.mem.eql(u8, resource_name, herdr_config_name))
    {
        return error.UnsealedResource;
    }
    const marker_index = std.mem.lastIndexOf(
        u8,
        executable_path,
        executable_marker,
    ) orelse return error.NotPackagedApplication;
    const bundle_path = executable_path[0..marker_index];
    const executable_name = executable_path[marker_index + executable_marker.len ..];
    if (!std.mem.endsWith(u8, bundle_path, ".app") or
        !std.mem.eql(u8, executable_name, host_name))
    {
        return error.NotPackagedApplication;
    }
    return std.fmt.bufPrint(
        output,
        "{s}/Contents/Resources/{s}",
        .{ bundle_path, resource_name },
    );
}

test "sealed resources resolve beside each other in the signed bundle" {
    const executable =
        "/Applications/Agent Factory.app/Contents/MacOS/agent-factory";
    var runtime_storage: [std.Io.Dir.max_path_bytes]u8 = undefined;
    var updater_storage: [std.Io.Dir.max_path_bytes]u8 = undefined;
    var config_storage: [std.Io.Dir.max_path_bytes]u8 = undefined;
    var herdr_config_storage: [std.Io.Dir.max_path_bytes]u8 = undefined;

    try std.testing.expectEqualStrings(
        "/Applications/Agent Factory.app/Contents/Resources/agent-factory-runtime",
        try sealedPath(executable, runtime_name, &runtime_storage),
    );
    try std.testing.expectEqualStrings(
        "/Applications/Agent Factory.app/Contents/Resources/updater-helper",
        try sealedPath(executable, updater_name, &updater_storage),
    );
    try std.testing.expectEqualStrings(
        "/Applications/Agent Factory.app/Contents/Resources/update-config-v1.json",
        try sealedPath(executable, update_config_name, &config_storage),
    );
    try std.testing.expectEqualStrings(
        "/Applications/Agent Factory.app/Contents/Resources/herdr-client.toml",
        try sealedPath(executable, herdr_config_name, &herdr_config_storage),
    );
}

test "sealed resolution rejects development paths and arbitrary resources" {
    var storage: [std.Io.Dir.max_path_bytes]u8 = undefined;
    try std.testing.expectError(
        error.NotPackagedApplication,
        sealedPath(
            "/workspace/zig-out/bin/agent-factory",
            updater_name,
            &storage,
        ),
    );
    try std.testing.expectError(
        error.NotPackagedApplication,
        sealedPath(
            "/tmp/example/Contents/MacOS/not-agent-factory",
            updater_name,
            &storage,
        ),
    );
    try std.testing.expectError(
        error.UnsealedResource,
        sealedPath(
            "/Applications/Agent Factory.app/Contents/MacOS/agent-factory",
            "../../replacement",
            &storage,
        ),
    );
}

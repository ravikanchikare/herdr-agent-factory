use std::fs;
use std::io::{BufReader, BufWriter};
use std::process::{Command, Stdio};

use ipc_contract::{Frame, Request, ResponseOutcome, Shutdown, read_frame, write_frame};
use serde_json::json;
use tempfile::TempDir;

#[test]
fn runtime_serves_the_versioned_protocol_over_stdio() {
    let data = TempDir::new().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_agent-factory-runtime"))
        .env("AGENT_FACTORY_DATA_DIR", data.path())
        .env("AGENT_FACTORY_TEST_IN_MEMORY_SECRETS", "1")
        // A runtime integration test must never subscribe to or control the
        // developer's live Herdr server.
        .env(
            "AGENT_FACTORY_HERDR_SOCKET",
            data.path().join("missing-herdr.sock"),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut writer = BufWriter::new(child.stdin.take().unwrap());
    let mut reader = BufReader::new(child.stdout.take().unwrap());

    assert!(matches!(
        read_frame(&mut reader).unwrap(),
        Some(Frame::Ready(_))
    ));

    write_frame(
        &mut writer,
        &Frame::Request(Request::new(
            "settings.setTheme",
            json!({ "theme": "dark" }),
        )),
    )
    .unwrap();
    assert!(matches!(
        read_frame(&mut reader).unwrap(),
        Some(Frame::Response(response))
            if matches!(response.outcome, ResponseOutcome::Success { ref result }
                if result["settings"]["theme"] == "dark" && result["revision"] == 1)
    ));
    assert!(matches!(
        read_frame(&mut reader).unwrap(),
        Some(Frame::Event(event))
            if event.topic == "settings.changed" && event.revision == 1
    ));
    write_frame(
        &mut writer,
        &Frame::Request(Request::new(
            "settings.setNotifications",
            json!({ "enabled": false }),
        )),
    )
    .unwrap();
    assert!(matches!(
        read_frame(&mut reader).unwrap(),
        Some(Frame::Response(response))
            if matches!(response.outcome, ResponseOutcome::Success { ref result }
                if result["settings"]["nativeNotifications"] == false
                    && result["revision"] == 2)
    ));
    assert!(matches!(
        read_frame(&mut reader).unwrap(),
        Some(Frame::Event(event)) if event.topic == "settings.changed"
    ));

    let request = Request::new("runtime.hello", json!({}));
    let request_id = request.id;
    write_frame(&mut writer, &Frame::Request(request)).unwrap();
    let response = read_frame(&mut reader).unwrap().unwrap();
    assert!(matches!(
        response,
        Frame::Response(response)
            if response.id == request_id
                && matches!(response.outcome, ResponseOutcome::Success { .. })
    ));

    let project_root = data.path().join("project");
    fs::create_dir(&project_root).unwrap();
    fs::write(project_root.join("README.md"), "Agent Factory").unwrap();
    write_frame(
        &mut writer,
        &Frame::Request(Request::new(
            "project.create",
            json!({
                "name": "Test project",
                "root": project_root,
                "trusted": true,
            }),
        )),
    )
    .unwrap();
    assert!(matches!(
        read_frame(&mut reader).unwrap(),
        Some(Frame::Response(_))
    ));
    assert!(matches!(
        read_frame(&mut reader).unwrap(),
        Some(Frame::Event(_))
    ));

    write_frame(
        &mut writer,
        &Frame::Request(Request::new(
            "file.read",
            json!({ "path": project_root.join("README.md") }),
        )),
    )
    .unwrap();
    assert!(matches!(
        read_frame(&mut reader).unwrap(),
        Some(Frame::Response(response))
            if matches!(
                response.outcome,
                ResponseOutcome::Success { ref result }
                    if result["content"] == "Agent Factory"
            )
    ));

    write_frame(
        &mut writer,
        &Frame::Shutdown(Shutdown {
            version: 1,
            reason: "test complete".into(),
        }),
    )
    .unwrap();
    assert!(child.wait().unwrap().success());

    let mut restarted = Command::new(env!("CARGO_BIN_EXE_agent-factory-runtime"))
        .env("AGENT_FACTORY_DATA_DIR", data.path())
        .env("AGENT_FACTORY_TEST_IN_MEMORY_SECRETS", "1")
        .env(
            "AGENT_FACTORY_HERDR_SOCKET",
            data.path().join("missing-herdr.sock"),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut restarted_writer = BufWriter::new(restarted.stdin.take().unwrap());
    let mut restarted_reader = BufReader::new(restarted.stdout.take().unwrap());
    assert!(matches!(
        read_frame(&mut restarted_reader).unwrap(),
        Some(Frame::Ready(_))
    ));
    write_frame(
        &mut restarted_writer,
        &Frame::Request(Request::new("snapshot.get", json!({}))),
    )
    .unwrap();
    assert!(matches!(
        read_frame(&mut restarted_reader).unwrap(),
        Some(Frame::Response(response))
            if matches!(response.outcome, ResponseOutcome::Success { ref result }
                if result["settings"]
                    == json!({
                        "theme":"dark",
                        "nativeNotifications":false,
                        "layout":{"inspectorPercent":28,"terminalPercent":24}
                    })
                    && result["revision"] == 3)
    ));
    write_frame(
        &mut restarted_writer,
        &Frame::Shutdown(Shutdown {
            version: 1,
            reason: "restart verified".into(),
        }),
    )
    .unwrap();
    assert!(restarted.wait().unwrap().success());
}

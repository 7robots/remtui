#![allow(dead_code)]
//! Shared fixtures: the fake binaries pointed at temp state files.

use std::sync::Arc;

use remtui::bear::BearClient;
use remtui::client::RemctlClient;

pub struct Fake {
    pub dir: tempfile::TempDir,
}

impl Fake {
    pub fn new() -> Fake {
        Fake {
            dir: tempfile::tempdir().expect("tempdir"),
        }
    }

    pub fn remctl_state(&self) -> String {
        self.dir
            .path()
            .join("demo.json")
            .to_string_lossy()
            .into_owned()
    }

    pub fn bear_state(&self) -> String {
        self.dir
            .path()
            .join("demo-bear.json")
            .to_string_lossy()
            .into_owned()
    }

    pub fn remctl_envs(&self) -> Vec<(String, String)> {
        vec![("REMTUI_FAKE_STATE".into(), self.remctl_state())]
    }

    pub fn client(&self) -> RemctlClient {
        RemctlClient::with_env(
            vec![env!("CARGO_BIN_EXE_fake-remctl").into()],
            self.remctl_envs(),
        )
    }

    /// A client whose flag writes fail, like a Mac without Automation access.
    pub fn client_failing_flags(&self) -> RemctlClient {
        let mut envs = self.remctl_envs();
        envs.push(("REMTUI_FAKE_FLAG_FAILS".into(), "1".into()));
        RemctlClient::with_env(vec![env!("CARGO_BIN_EXE_fake-remctl").into()], envs)
    }

    pub fn bear(&self) -> BearClient {
        BearClient::with_env(
            vec![env!("CARGO_BIN_EXE_fake-bearcli").into()],
            vec![("REMTUI_FAKE_BEAR_STATE".into(), self.bear_state())],
        )
    }

    pub fn shared_client(&self) -> Arc<RemctlClient> {
        Arc::new(self.client())
    }

    pub fn read_bear_state(&self) -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(self.bear_state()).unwrap()).unwrap()
    }

    pub fn read_remctl_state(&self) -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(self.remctl_state()).unwrap()).unwrap()
    }
}

pub fn numeric_id(result: &Option<serde_json::Value>) -> i64 {
    result
        .as_ref()
        .and_then(|r| r.get("numericId"))
        .and_then(serde_json::Value::as_i64)
        .expect("numericId")
}

pub fn status(result: &Option<serde_json::Value>) -> String {
    result
        .as_ref()
        .and_then(|r| r.get("status"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string()
}

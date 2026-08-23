use serde::{Deserialize, Serialize};

use crate::UpdateError;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum UpdateState {
    Idle,
    Checking,
    Available { version: String },
    AwaitingConfirmation { version: String },
    Confirmed { version: String },
    Downloading { version: String },
    Verifying { version: String },
    Staged { version: String, path: String },
    Installing { version: String },
    ReadyToRestart { version: String },
    Failed { message: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateStateMachine {
    state: UpdateState,
}

impl Default for UpdateStateMachine {
    fn default() -> Self {
        Self {
            state: UpdateState::Idle,
        }
    }
}

impl UpdateStateMachine {
    pub fn state(&self) -> &UpdateState {
        &self.state
    }

    pub fn begin_check(&mut self) -> Result<(), UpdateError> {
        match self.state {
            UpdateState::Idle | UpdateState::Failed { .. } => {
                self.state = UpdateState::Checking;
                Ok(())
            }
            _ => Err(UpdateError::InvalidTransition),
        }
    }

    pub fn no_update(&mut self) -> Result<(), UpdateError> {
        self.require(|state| matches!(state, UpdateState::Checking))?;
        self.state = UpdateState::Idle;
        Ok(())
    }

    pub fn update_available(&mut self, version: impl Into<String>) -> Result<(), UpdateError> {
        self.require(|state| matches!(state, UpdateState::Checking))?;
        self.state = UpdateState::Available {
            version: version.into(),
        };
        Ok(())
    }

    pub fn request_confirmation(&mut self) -> Result<(), UpdateError> {
        let version = self.version_if(|state| matches!(state, UpdateState::Available { .. }))?;
        self.state = UpdateState::AwaitingConfirmation { version };
        Ok(())
    }

    pub fn confirm(&mut self, version: &str) -> Result<(), UpdateError> {
        let expected =
            self.version_if(|state| matches!(state, UpdateState::AwaitingConfirmation { .. }))?;
        if expected != version {
            return Err(UpdateError::ConfirmationMismatch);
        }
        self.state = UpdateState::Confirmed { version: expected };
        Ok(())
    }

    pub fn begin_download(&mut self) -> Result<(), UpdateError> {
        let version = self.version_if(|state| matches!(state, UpdateState::Confirmed { .. }))?;
        self.state = UpdateState::Downloading { version };
        Ok(())
    }

    pub fn begin_verification(&mut self) -> Result<(), UpdateError> {
        let version = self.version_if(|state| matches!(state, UpdateState::Downloading { .. }))?;
        self.state = UpdateState::Verifying { version };
        Ok(())
    }

    pub fn staged(&mut self, path: impl Into<String>) -> Result<(), UpdateError> {
        let version = self.version_if(|state| matches!(state, UpdateState::Verifying { .. }))?;
        self.state = UpdateState::Staged {
            version,
            path: path.into(),
        };
        Ok(())
    }

    pub fn begin_install(&mut self) -> Result<(), UpdateError> {
        let version = self.version_if(|state| matches!(state, UpdateState::Staged { .. }))?;
        self.state = UpdateState::Installing { version };
        Ok(())
    }

    pub fn ready_to_restart(&mut self) -> Result<(), UpdateError> {
        let version = self.version_if(|state| matches!(state, UpdateState::Installing { .. }))?;
        self.state = UpdateState::ReadyToRestart { version };
        Ok(())
    }

    pub fn fail(&mut self, message: impl Into<String>) {
        self.state = UpdateState::Failed {
            message: message.into(),
        };
    }

    fn require(&self, predicate: impl FnOnce(&UpdateState) -> bool) -> Result<(), UpdateError> {
        predicate(&self.state)
            .then_some(())
            .ok_or(UpdateError::InvalidTransition)
    }

    fn version_if(
        &self,
        predicate: impl FnOnce(&UpdateState) -> bool,
    ) -> Result<String, UpdateError> {
        self.require(predicate)?;
        match &self.state {
            UpdateState::Available { version }
            | UpdateState::AwaitingConfirmation { version }
            | UpdateState::Confirmed { version }
            | UpdateState::Downloading { version }
            | UpdateState::Verifying { version }
            | UpdateState::Staged { version, .. }
            | UpdateState::Installing { version }
            | UpdateState::ReadyToRestart { version } => Ok(version.clone()),
            _ => Err(UpdateError::InvalidTransition),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_requires_matching_explicit_confirmation() {
        let mut state = UpdateStateMachine::default();
        state.begin_check().unwrap();
        state.update_available("1.1.0").unwrap();
        assert_eq!(state.begin_download(), Err(UpdateError::InvalidTransition));
        state.request_confirmation().unwrap();
        assert_eq!(
            state.confirm("1.2.0"),
            Err(UpdateError::ConfirmationMismatch),
        );
        state.confirm("1.1.0").unwrap();
        state.begin_download().unwrap();
        state.begin_verification().unwrap();
        state.staged("/private/tmp/update").unwrap();
        state.begin_install().unwrap();
        state.ready_to_restart().unwrap();
        assert_eq!(
            state.state(),
            &UpdateState::ReadyToRestart {
                version: "1.1.0".to_owned(),
            },
        );
    }

    #[test]
    fn invalid_transitions_fail_closed() {
        let mut state = UpdateStateMachine::default();
        assert_eq!(
            state.ready_to_restart(),
            Err(UpdateError::InvalidTransition)
        );
        state.fail("network unavailable");
        state.begin_check().unwrap();
        state.no_update().unwrap();
        assert_eq!(state.state(), &UpdateState::Idle);
    }
}

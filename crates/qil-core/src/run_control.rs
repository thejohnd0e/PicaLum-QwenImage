use std::future::Future;

use tokio::sync::watch;
use tokio::time::{sleep_until, Instant};

#[derive(Debug, PartialEq, Eq)]
pub enum RunError {
    Cancelled,
    TimedOut,
    CancellationUnconfirmed,
    TimeoutUnconfirmed,
    Request(String),
}

impl From<String> for RunError {
    fn from(value: String) -> Self {
        Self::Request(value)
    }
}

impl From<&str> for RunError {
    fn from(value: &str) -> Self {
        Self::Request(value.to_string())
    }
}

pub struct RunControl {
    cancel: watch::Receiver<bool>,
    deadline: Instant,
}

impl RunControl {
    pub fn new(cancel: watch::Receiver<bool>, deadline: Instant) -> Self {
        Self { cancel, deadline }
    }

    pub async fn race<T, F>(&mut self, work: F) -> Result<T, RunError>
    where
        F: Future<Output = Result<T, String>>,
    {
        if *self.cancel.borrow() {
            return Err(RunError::Cancelled);
        }

        let deadline = self.deadline;
        tokio::select! {
            biased;
            () = wait_for_cancel(&mut self.cancel) => Err(RunError::Cancelled),
            () = sleep_until(deadline) => Err(RunError::TimedOut),
            result = work => result.map_err(RunError::Request),
        }
    }
}

async fn wait_for_cancel(receiver: &mut watch::Receiver<bool>) {
    loop {
        if *receiver.borrow_and_update() {
            return;
        }
        if receiver.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::sync::watch;
    use tokio::time::Instant;

    use super::{RunControl, RunError};

    #[tokio::test]
    async fn run_control_returns_success_when_work_finishes() {
        let (_cancel_tx, cancel_rx) = watch::channel(false);
        let mut control = RunControl::new(cancel_rx, Instant::now() + Duration::from_secs(5));

        let result = control.race(async { Ok::<_, String>(42) }).await;

        assert_eq!(result, Ok(42));
    }

    #[tokio::test]
    async fn run_control_returns_cancelled_when_signal_arrives() {
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let mut control = RunControl::new(cancel_rx, Instant::now() + Duration::from_secs(5));
        cancel_tx.send(true).unwrap();

        let result = control
            .race(std::future::pending::<Result<(), String>>())
            .await;

        assert_eq!(result, Err(RunError::Cancelled));
    }

    #[tokio::test]
    async fn run_control_cancels_in_flight_work() {
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let control = RunControl::new(cancel_rx, Instant::now() + Duration::from_secs(5));
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let mut control = control;
            control
                .race(async move {
                    started_tx.send(()).unwrap();
                    std::future::pending::<Result<(), String>>().await
                })
                .await
        });
        started_rx.await.unwrap();
        cancel_tx.send(true).unwrap();

        assert_eq!(task.await.unwrap(), Err(RunError::Cancelled));
    }

    #[tokio::test]
    async fn run_control_returns_timeout_when_deadline_has_passed() {
        let (_cancel_tx, cancel_rx) = watch::channel(false);
        let mut control = RunControl::new(cancel_rx, Instant::now() - Duration::from_secs(1));

        let result = control
            .race(std::future::pending::<Result<(), String>>())
            .await;

        assert_eq!(result, Err(RunError::TimedOut));
    }

    #[tokio::test]
    async fn run_control_preserves_request_errors() {
        let (_cancel_tx, cancel_rx) = watch::channel(false);
        let mut control = RunControl::new(cancel_rx, Instant::now() + Duration::from_secs(5));

        let result = control
            .race(async { Err::<(), _>(String::from("engine failed")) })
            .await;

        assert_eq!(
            result,
            Err(RunError::Request(String::from("engine failed")))
        );
    }
}

use std::cell::RefCell;
use futures::sync::oneshot;
use futures::sync::oneshot::Receiver;
use futures::Stream;
use futures::future::{self, Future};
use tokio::runtime::Runtime;
use tokio::runtime::TaskExecutor;
use exit_future::Exit;
use tokio::timer::Interval;
use std::time::{Instant, Duration};
use std::thread::sleep;
use log::warn;
use signal_hook::{iterator::Signals, SIGUSR1};

fn main() {
    execute();
}

fn execute() {

    let outer_exit = OuterExit;

    start_service(outer_exit);

}

fn start_service(outer_exit: OuterExit){

    println!("Start service");

    let mut runtime = Runtime::new().map_err(|e| format!("{:?}", e)).expect("qed");
    let executor = runtime.executor();

    let (exit_send, exit) = exit_future::signal();

    start_task(exit, executor);

    runtime.block_on(outer_exit.into_exit());

    exit_send.fire();

    println!("Finish service");
}

fn start_task(exit: Exit, executor: TaskExecutor){

    println!("Start task");

    let task = Interval::new(Instant::now(), Duration::from_secs(10)).for_each(move |_instant| {

        println!("task loop start");

        sleep(Duration::from_secs(3));

        println!("task loop finish");

        Ok(())
    }).map_err(|e| warn!("Error: {:?}", e));

    executor.spawn(exit.until(task).map(|_| ()));

    println!("Finish task");
}

pub trait IntoExit {
    /// Exit signal type.
    type Exit: Future<Item=(),Error=()> + Send + 'static;
    /// Convert into exit signal.
    fn into_exit(self) -> Self::Exit;
}

pub struct OuterExit;
impl IntoExit for OuterExit {
    type Exit = future::MapErr<oneshot::Receiver<()>, fn(oneshot::Canceled) -> ()>;
    fn into_exit(self) -> Self::Exit {
        // can't use signal directly here because CtrlC takes only `Fn`.
        let (exit_send, exit) = oneshot::channel();

        let exit_send_cell = RefCell::new(Some(exit_send));
        ctrlc::set_handler(move || {
            if let Some(exit_send) = exit_send_cell.try_borrow_mut().expect("signal handler not reentrant; qed").take() {
                exit_send.send(()).expect("Error sending exit notification");
            }
        }).expect("Error setting Ctrl-C handler");

        exit.map_err(drop)
    }
}
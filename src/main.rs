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
use signal_hook::{iterator::Signals, SIGUSR1, SIGINT, SIGTERM};
use std::thread;

fn main() {

    let mut run = true;

    while run{
        let exit_item = execute();
        run = match exit_item{
            ExitItem::Stop => false,
            ExitItem::Restart => true,
        }
    }
}

fn execute() -> ExitItem {

    let outer_exit = OuterExit;

    start_service(outer_exit)

}

fn start_service<E: IntoExit>(outer_exit: E) -> E::Item{

    println!("Start service");

    let mut runtime = Runtime::new().map_err(|e| format!("{:?}", e)).expect("qed");
    let executor = runtime.executor();

    let (exit_send, exit) = exit_future::signal();

    start_task(exit, executor);

    let exit_item = runtime.block_on(outer_exit.into_exit()).unwrap();

    exit_send.fire();

    println!("Finish service");

    exit_item
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
    type Item: Send + 'static;
    /// Exit signal type.
    type Exit: Future<Item=Self::Item,Error=()> + Send + 'static;
    /// Convert into exit signal.
    fn into_exit(self) -> Self::Exit;
}

#[derive(Debug)]
pub enum ExitItem{
    Stop,
    Restart,
}

pub struct OuterExit;
impl IntoExit for OuterExit {
    type Item = ExitItem;
    type Exit = future::MapErr<oneshot::Receiver<Self::Item>, fn(oneshot::Canceled) -> ()>;
    fn into_exit(self) -> Self::Exit {
        // can't use signal directly here because CtrlC takes only `Fn`.
        let (exit_send, exit) = oneshot::channel();

        let exit_send_cell = RefCell::new(Some(exit_send));

        let signals = Signals::new(&[SIGUSR1, SIGINT, SIGTERM]).unwrap();

        thread::spawn(move || {
            for sig in signals.forever() {
                println!("Received signal {:?}", sig);

                let item = match sig{
                    SIGUSR1 => ExitItem::Restart,
                    _ => ExitItem::Stop,
                };

                if let Some(exit_send) = exit_send_cell.try_borrow_mut().expect("signal handler not reentrant; qed").take() {
                    exit_send.send(item).expect("Error sending exit notification");
                }
            }
        });

        exit.map_err(drop)
    }
}
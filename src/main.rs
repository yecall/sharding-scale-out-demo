use futures::sync::oneshot;
use futures::sync::oneshot::{Sender};
use futures::Stream;
use futures::future::{self, Future};
use tokio::runtime::Runtime;
use tokio::runtime::TaskExecutor;
use exit_future::Exit;
use tokio::timer::{Interval, Delay};
use std::time::{Instant, Duration};
use std::thread::sleep;
use log::warn;
use signal_hook::{iterator::Signals, SIGUSR1, SIGINT, SIGTERM};
use std::thread;
use std::sync::{Arc, Mutex};
use std::fmt::Debug;

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

fn start_service<E>(outer_exit: E) -> <E::SendExit as SendExit>::Item
where E : IntoExit<SendExit=OuterSendExit<ExitItem>>{

    println!("Start service");

    let mut runtime = Runtime::new().map_err(|e| format!("{:?}", e)).expect("qed");
    let executor = runtime.executor();

    let (exit_send, exit) = exit_future::signal();

    let (outer_exit, outer_send_exit) = outer_exit.into_exit();

    start_restarter::<E>(exit.clone(), &executor, outer_send_exit);

    start_task(exit, &executor);

    let exit_item = runtime.block_on(outer_exit).unwrap();

    exit_send.fire();

    println!("Finish service");

    exit_item
}

fn start_restarter<E: IntoExit>(exit: Exit, executor: &TaskExecutor, outer_send_exit: E::SendExit)
    where E : IntoExit<SendExit=OuterSendExit<ExitItem>>{

    println!("Start restarter");

    let task = Delay::new(Instant::now() + Duration::from_secs(15)).then(move |_x| {

        println!();
        println!("Restart");

        outer_send_exit.send(ExitItem::Restart);

        Ok(())
    });

    executor.spawn(exit.until(task).map(|_| ()));

    println!("Finish restarter");

}

fn start_task(exit: Exit, executor: &TaskExecutor){

    println!("Start task");

    let task = Interval::new(Instant::now(), Duration::from_secs(10)).for_each(move |_instant| {

        println!("task loop start");

        sleep(Duration::from_secs(3));

        println!("task loop finish");

        Ok(())
    }).map_err(|e| warn!("Error: {:?}", e));

    executor.spawn(exit.until(task).map(|_| ()));

}

pub trait SendExit{
    type Item: Send + 'static;
    fn send(&self, item: Self::Item);
}

pub trait IntoExit {
    type SendExit: SendExit + Send + 'static;
    /// Exit signal type.
    type Exit: Future<Item=<Self::SendExit as SendExit>::Item,Error=()> + Send + 'static;
    /// Convert into exit signal.
    fn into_exit(self) -> (Self::Exit, Self::SendExit);
}

#[derive(Debug)]
pub enum ExitItem{
    Stop,
    Restart,
}

pub struct OuterSendExit<Item>{
    sender: Arc<Mutex<Option<Sender<Item>>>>,
}

impl<Item: Send + Debug + 'static> SendExit for OuterSendExit<Item> {
    type Item = Item;
    fn send(&self, item: Self::Item){
        if let Some(exit_send) = self.sender.lock().unwrap().take() {
            exit_send.send(item).expect("Error sending exit notification");
        }
    }
}

pub struct OuterExit;
impl IntoExit for OuterExit {
    type SendExit = OuterSendExit<ExitItem>;
    type Exit = future::MapErr<oneshot::Receiver<<Self::SendExit as SendExit>::Item>, fn(oneshot::Canceled) -> ()>;
    fn into_exit(self) -> (Self::Exit, Self::SendExit) {
        // can't use signal directly here because CtrlC takes only `Fn`.
        let (exit_send, exit) = oneshot::channel();

        let exit_send_cell = Arc::new(Mutex::new(Some(exit_send)));

        let exit_send_cell_clone = exit_send_cell.clone();

        let signals = Signals::new(&[SIGUSR1, SIGINT, SIGTERM]).unwrap();

        thread::spawn(move || {
            for sig in signals.forever() {
                println!("Received signal {:?}", sig);

                let item = match sig{
                    SIGUSR1 => ExitItem::Restart,
                    _ => ExitItem::Stop,
                };

                if let Some(exit_send) = exit_send_cell.lock().unwrap().take() {
                    exit_send.send(item).expect("Error sending exit notification");
                }
            }
        });

        (exit.map_err(drop), OuterSendExit{sender: exit_send_cell_clone})
    }
}
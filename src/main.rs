use std::cell::RefCell;
use futures::sync::oneshot;
use futures::sync::oneshot::Receiver;
use futures::Stream;
use futures::future::Future;
use tokio::runtime::Runtime;
use tokio::runtime::TaskExecutor;
use exit_future::Exit;
use tokio::timer::Interval;
use std::time::{Instant, Duration};
use std::thread::sleep;
use log::warn;

fn main() {

    let mut runtime = Runtime::new().map_err(|e| format!("{:?}", e)).expect("qed");
    let executor = runtime.executor();

    let ctrlc_exit = get_ctrlc_exit();

    let (exit_send, exit) = exit_future::signal();

    start_service(exit, executor);

    runtime.block_on(ctrlc_exit).expect("qed");

    exit_send.fire();

    println!("End");

}

fn get_ctrlc_exit() -> Receiver<()>{

    let (ctrlc_send, ctrlc_exit) = oneshot::channel();
    let ctrlc_send = RefCell::new(Some(ctrlc_send));
    ctrlc::set_handler(move || {
        println!("Ctrl+C");
        ctrlc_send.borrow_mut().take().unwrap().send(()).expect("Error sending exit notification");
    }).expect("Error setting Ctrl-C handler");


    ctrlc_exit
}

fn start_service(exit: Exit, executor: TaskExecutor){

    println!("Start service");

    let task = Interval::new(Instant::now(), Duration::from_secs(10)).for_each(move |_instant| {

        println!("loop start");

        sleep(Duration::from_secs(3));

        println!("loop finish");

        Ok(())
    }).map_err(|e| warn!("Error: {:?}", e));

    executor.spawn(exit.until(task).map(|_| ()));

}

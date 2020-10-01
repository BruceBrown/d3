#d3 -- Short for Data Drive Dispatch is a server framework

## Some historical perspective
A few monts back I decided to learn Go, then Rust. When it came to learning Rust, I did a quick game, played with
webassembly, and then decided to build a server to find out just how good Rust is for systems programming.
Over the past 30 plus years I've worked on server development; newpaper servers, data base servers, email servers,
meeting servers, etc. I've done the "how do we avoid deadlocks" game too many times. I've had to apply all kinds
of ticks to squeeze the most out of server performance. Armed with all that knowledge -- I've earned these gray
hairs -- this ismy take on a different, if not better approach to the problem.

What are those problems? I've already mentioned performance and deadlocks. There's life-cycle stuff -- some of
the code being run today, I wrote 30 years ago. There are model changes which up at the worse times, often
with something that in order to work has to wrap a bunch of somethings that were never intended to be wrapped.

## What I've built
The solution I've come up with is to have all objects communicate with each other, not through an instance,
but through a messages queue. And while such systems have existed for decades (I've written more than one),
this one has a slightly differnet twist, we're going to express all of the messages -- or instructions as
enum variants. We're going to send them through a channel. We're going to monitor every receiver and execute
them when a message is received. The entire system is asnchronous, with regards to objects. It is the data,
passed as variants that provides any needed synchronization. And we're going to use rust and heavy rely upon
the borrow checker to ensure we don't screw things up.

I must say that I'm impressed by both the performance of Rust and the community supporting it. I could not have built
what I did over the past two month without the support of the comunity or without some standout crates, such as
Crossbeam, Mio, AtomicRefCell, and SmartDefault. It has enabled me to write this entire framework quickly
and without the use of `Unsafe`. Not that its a bad thing, but it allowed me to focus on what I needed to implement
rather that trying to figure out the bast/fastest/safest way to do something. I'll optimize later, as premature
optimization is a young person's problem.

This is still a work in progress. However, its advanced enough that I'm publishing it.
## Theory of Operation
The central focus is centered around data driven dispatch. What I mean by that is having a set of objects which
are driven by receiving data. There are several key elements:
* Expressing an instruction set, in the form of variants.
* Creating a set of machines that implement the instruction set.
* Adding the machine to a collective
* allowing access, by providing a sender to other machines.
When a machine is added to the collective, it provides a reciver, and is given a sender. That sender can be cloned
and given to other machines. When data becomes available, the machine is sceduled and execution begins.

One of the first tests I ran was creating a daisy chain of 4000 machines and sending 200 messages into the first,
and getting 200 out of the last, this proved that the scheduler and executor are well behaved. Next, I had 1 machine
fan 200 messages out to 4000 machines which then sent all there messages to a single machine. This exercised the
parking of machines which became blocked on a full send queue -- a further test of the sceduler and executor.

There is a thin TCP layer, which, thanks to Mio, provides async I/O to the network. I'm planning on adding UDP
in thenear future.

Lastly, I added a framework on top of the raw machines, which implements a Coordinator, Component, Connection
model. This is intended to provide ochestration and scalability.

A bit of polishing, in the form of logging and configuration was added, and after some cleanup, I'm publishing
it. I've include an EchoServer and ChatServer, both of which I used to get a feeling for how easy or difficult
it is to extend the server with services.

## Example of an Instruction set
A MachineImpl derive macro has been provided to support instruction sets.
All you need to do is add it to an enum, like this:
``` Rust
/// These are the event types which are exchanged for testing
#[allow(dead_code)]
#[derive(Debug, MachineImpl, Clone, Eq, PartialEq)]
pub enum TestMessage {
    Test,
    TestData(usize),
    TestStruct(TestStruct),
    TestCallback(Sender<TestMessage>, TestStruct),
    AddSender(Sender<TestMessage>),
    Notify(Sender<TestMessage>, usize),
}

/// Test structure for callback and exchange
#[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
pub struct TestStruct {
    pub from_id: usize,
    pub received_by: usize,
}
```

## example of a Machine
Admittedly, it doesn't do much, `receive` is how instructions are supplied to the machine
``` Rust
struct Example {}
impl Machine<TestMessage> for Example {
fn disconnected(&self) {}
fn receive(&self, cmd: TestMessage) {}
}
```

## example of adding a Machine to the Collective
``` Rust
/// Add it to the Collective
let (_, sender) = executor::connect(Example{});
/// send it a command
sender.send(TestMessage::Test);
```
## example of a Machine that understands two different instruction sets
``` Rust
struct Example {}
impl Machine<TestMessage> for Example {
fn disconnected(&self) {}
fn receive(&self, cmd: TestMessage) {}
}
impl Machine<StartStop> for Example {
fn disconnected(&self) {}
fn receive(&self, cmd: StartStop) {}
}

/// Add it to the Collective
let (m, test_message_sender) = executor::connect::<_,TestMessage>(Example{});
let start_stop_sender = executor::and_connect::<_,StarStop>(&m);
```
## Starting and Stopping the Server
The server is started by calling `executor::start_server()` and stopped by
calling `executor::stop_server()`. Once started, the network can be started
by calling `network::start_network()` and stopped by calling `network::stop_network()`.

I've provided an `EchoServer` and `ChatServer` as example services, along with
a configuration that enables both simultaneously. I've also provideda `Forwarder`,
which I use for performance testing.

That's it in a nut shell. At this point I'm looking for feed back, and am looking
at performance.

# d3 -- Revision ChangeLog

## Performance

* Changed benchmarks to run longer. They were executing in a few milliseconds, which made them difficult to compare.
* Re-worked how tasks are managed, removing the Task that wrapped a machine.
* Re-worked the executor.
* Evaluating closure call vs trait object call for machine methods. Closure looks like a winner.
* Removed Mutex from a most Arc<Mutex<T>>, in favor of Arc<Atomic>.
* Replaced Weak usage with Arc, avoiding the upgrade and downgrade overhead on parked senders.

## 12-Nov-2020 Release 0.1.3 to crates.io

* Release d3, d3-derive, d3-core, d3-components. Recommend using d3, which re-exports the others.

| Crate         | Version | Description |
|--------------:|--------:|:------------|
| d3 | v0.1.3 | Combined crate, re-exporting d3-derive, d3-core, d3-components |
| d3-derive | v0.1.3 | MachineImpl derive macro |
| d3-core | v0.1.3 | Core layer, includes scheduler and executor |
| d3-components | v0.1.3 | Component layer, includes Network, Settings Coordinator, Component and Connetor |

## SuperSlab and Conway's Game of Life

* Move version to v0.1.3 for all crates.
* Switch from slab::Slab to super_slab::SuperSlab to handle machine growth
* Added an example, Conway's Game of Life, as an example of using machines as game elements.
* Fixed a data-race that would occasionally leave machines alive with no Senders.

## Performance and UDP

* Switch from std::sync::{Mutex, RWLock} to parking_lot versions, they are slightly faster.
* Finally, got around to adding UDP support to the network.
* Added new example of udp echo service.

## 29-Oct-2020 Release to crates.io

| Crate         | Version | Description |
|--------------:|--------:|:------------|
| d3 | v0.1.2 | Combined crate, re-exporting d3-derive, d3-core, d3-components |
| d3-derive | v0.1.2 | MachineImpl derive macro |
| d3-core | v0.1.2 | Core layer, includes scheduler and executor |
| d3-components | v0.1.2 | Component layer, includes Network, Settings Coordinator, Component and Connetor |


## Clean-up, prepare for combined d3 release, re-implement sched
* Data shows crossbeam select, while great for a few channels, doesn't scale well into the 1000s; re-implemented sched w/out select.
* Enhanced examples. Re-arranged the folders. Changed settings values from enum to string.
* Large re-working of scheduler, executor and channel. The result is a 4x-10x performance improvement within d3-core.

## 17-Oct-2020 Release to crates.io
problems with README, links, and badges... This forced a new release.
0.1.1 -- d3-derive released.
0.1.1 -- d3-core released.
0.1.0 -- d3-components released.

0.1.0 -- d3-derive released.
0.1.0 -- d3-core released.


## Docs, Packaging and update
Updated dependencies, bumped up to crossbeam 0.8 and crossbeam-channel 0.5.

## Docs, Inline Examples, Clippy and Fmt
Quite a few changes however, its mostly formatting, adding and cleaning up documentation. The actual implementation is ony slightly changed.

## Finally, executor threads can now idle
Previously, the executor threads would yield when a cycle performed no work. Now, it will use a backoff to yeild and when the backoff has determined that its completed, the thread will park. This requires that the scheduler wake it when it schedules work. The downside of this is that the schdeuler needs to know if there are executors sleeping, which becomes an atomic shared between executors and the scheduler.

### Added a trivial Monitor Service
The service will periodically display statistics from the executor and scheduler. You'll find it in the new examples folder. Connect with nc, telnet, or your favorite tcp tool.
### Added a trivial alice-service
The service is an http/html service that can be driven by a browser. Its a form where you can initialize, start, stop, and wave good-bye to Alice. Connect with your favorite browser for seconds of fun.

## More Cleanup and Reorg
Added integration tests and benches. Move Forwarder, DaisyChain, FaninFanout and ChaosMonkey into their own area; d3-test-driver. Where they can be referenced by benches and tests. Move the test server from /src to d3-test-server, which seems like a more fitting place.
Cleaned up some stuff pointed out by clippy. Ran fmt on everything... some things it does I just don't like, but all in all its a boon. Went through the various inline tests and removed or cleaned them up. Need to make a pass over the internal docs and see if I can improve them to the point of being worth the trouble to read. Renamed chat-server and echo-server to chat-service and echo-server. Certainly, they could be called servers, but they also coexist in a server, so service seems like a more appropriate tagline.

## Chaos Monkey Added to Forwarder for Testing
The daisy-chain test is essentially a pulse wave though all the machine.
The fanout-fanin test is a single shock wave to the fanin, where many of the fanout will block on send.
The chaos-monkey is more random, when a forwarder receives a ChaosMonkey instruction it modifes it and either sends it to a random machine, including itself, or to the notifier if it is complete. The modification is to increment the count until it hits an inflection point, once that is hit the modification is to decrement the instuction count. When the count hits 0 a notification is sent. This should stress the scheduler and executor as a large number of random machines will try to send messages to each other.

Additionally, added unbound_queue option to forwarder, allowing for use of unbound queues -- dangerous on a server. The daisy-chain, with 12 machines, sending 1 message with a multiplier of 5 will send 48,828,125 messages. Add an additional, 13th machine and 244,140,625 messages are sent.

Took some time to get inline UTs working again. They had become stale as a few interfaces changed.

## Scheduler Reorg, Goal is to Halve the Time Spent in Scheduler
Changed out u128 in Machine (and elsewhere for a Uuid). Added a key: usize to the machine, and elsewhere. The key is set when storing into the collective. Replaced HashMap with Slab for storing machines. Replaced HashMap with IndexMap for storing Select index to machine key mapping.

Now for the testing. We're using the `daisy-chain` test with:
'''
timeout = 600
machines = 4000
messages = 400
forwarding_multiplier = 1
iterations = 10
'''
Prior to the changes the scheduler stats are:
'''
SelectStats {
    new_time: 798.91µs,
    rebuild_time: 2.520844ms,
    resched_time: 81.277232ms,
    select_time: 534.238318ms,
    total_time: 14.484425832s,
    empty_select: 4,
    selected_count: 519806,
}

After replacing HashMap with Slab and IndexMap, replacing some Arc<> with Box<>, adding an Arc<> wrapper for storage, creating tasks, and storing to TLS, we're down to this:
'''
SelectStats {
    new_time: 734.083µs,
    rebuild_time: 1.034533ms,
    resched_time: 13.506725ms,
    select_time: 230.381141ms,
    total_time: 9.752679423s,
    empty_select: 6,
    selected_count: 282208,
}
'''
After some unanticipated changes, we're down to this:
SchedStats {
    maint_time: 1.35025ms,
    new_time: 718.918µs,
    rebuild_time: 336.844µs,
    resched_time: 1.36715ms,
    select_time: 24.69267ms,
    total_time: 5.967185104s,
    empty_select: 38,
    selected_count: 50197,
    primary_select_count: 4122,
    slow_select_count: 4004,
    fast_select_count: 42071,
}
'''
As it turns out there were a few suprises. The select loop, in an effort to be fair, seldom selected the primary receiver, which is where the executor threads send machines back to be waited upon. Consequently, there was some starvation. Fixing that meant completing the suppor for a wait_queue, which is now done. Much of the low-level schedule consists of a select, a packaging of a task, sending it to the executor and waiting for it to be returned, where it is added to the select list. That exposed the next bottleneck, where a layer above has to interface with rebuidling the select. To remedy that, there's now a secondary select list built by the selector.

## Reorganized d3-lib into d3-core
Now that things are a bit more stable, `d3-lib`, which is built as a set of libraries, needs some help. Its a bit pointless to have the netsted library structure, as you need it all for things to work. So, things that were previously libraries under `d3-lib` are now just folders under `d3-core/src`. That rippled some changes upward,
however the code remains the same, just the names and locations moved.

As part of the change `d3-derive` was pulled up as a sibling of `d3-core`. If Rust ever allows macros in regular libraries, then it may move back under core. The test instruction set was also moved up as a sibling, as it is dependent upon the `#[derive(MachineImpl)]` and the underlying support in `d3-core`. It is now named `d3-dev-instruction-sets`.

Finally, I worked on the executor a bit more. Mostly a code cleanup and refactoring. Executror state and idle time is now exposed. I was hoping it would allow for better decision making on spawnnig threads when the current set of executors are blocked on send and they exhaust their backoff period. Its not an ideal solution, however if additional threads aren't spawned the entire system risks a lockup as receivers, which drain queus aren't able to run due to writes blocking all the executors.

Next on the docket is to cleanup integrated tests, which haven't kept up with recent code changes. Also, need to add UDP support.

## Forwarder Multiplier and Executor Fixes
First revision... added this file.

Added multiplier to Forwarder. This allows you to tell the Forwarder how many messages it should forward for every message it receives. This is a geometric hit to the executor and exposed a problem that I was having a difficult time reproducing: a sender that keeps filling the  message queue without returning back to the receive call. This potentially blocks an executor. For example, having a daisy-chain of 10 machines, with each machine having a multiplier of 5, will have that last notifier receiving 1,953,125 messages.

While spinning up new executors isn't a good thing, having a system grind to a halt is worse. So, we'll spin up a new executor and let the blocked one die after it completes its send. This also shows how important sizing is. Add a larger queue depth, and the problem goes away. Add more executors, and it goes away as well.

To be added: Find a way of determing if some executors are idle when blocking occurs, and if so, don't spin up a new executor as it will be of no value.

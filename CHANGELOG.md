# d3 -- Revision History
## Scheduler Reorg, Goal is to Halve the Time Spent in Scheduler
Changed out u128 in Machine (and elsehwere for a Uuid). Added a key: usize to the machine, and elsewhere. The key is set when storing into the collective. Replaced HashMap with Slab for storing machines. Replaced HashMap with IndexMap for storing Select index to machine key mapping.

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
After some unsuspecting changes, we're donw to this:
SchedStats {
    maint_time: 1.333007ms,
    new_time: 677.406µs,
    rebuild_time: 2.41147ms,
    resched_time: 11.907933ms,
    select_time: 25.676306ms,
    total_time: 6.402703662s,
    empty_select: 5,
    selected_count: 121486,
}
'''
As it turns out there were a few suprises. The select loop, in an effort to be fair, seldom selected the primary receiver, which is where the executor threads send machines back to be waited upon. Consequently, there was some starvation. Fixing that proved to be interesting while I've got a working fix, it needs a bit more refining. One of the ramifications is that select is no lnger for a fixed period of time, its for something less than 20ms depending upon how old the queued primary messages are. The goal being to balance between starvation and too often rebuilding the select table, which requires an examination of every machine's state. This brings up the question: Should there be a fast-queue for machines that seem to be hot vs machines which are cold. The idea being that if you could endd up with a better balance of select and not have to rebuild as often, its a bit of a pain due to haveing to track which machines are on the cold select and which are on the hot select. 

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

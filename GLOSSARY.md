# RazerBatteryTaskbar

A Windows tray app that shows the battery level of a Razer wireless mouse. This glossary covers how and when the tray reads the battery.

## Language

### Reading the battery

**Fresh battery read**:
A read in which the transport succeeded and the primary device answered with a battery level this time. A battery value kept from an earlier read while the mouse sleeps is not a fresh battery read.
_Avoid_: real read, successful poll

**Refresh schedule**:
The rules that decide when the tray next reads the battery: the regular poll, post-resume recovery and device-change rescans together.
_Avoid_: poll timer, polling interval (the interval is one part of the schedule)

### Recovering after a disruption

**Post-resume recovery**:
The short series of extra reads after Windows wakes from sleep or hibernate, while the receiver re-pairs with the mouse. It ends at the first fresh battery read or after its last attempt.
_Avoid_: resume refresh, wake retry

**Device-change rescan**:
The read that follows a burst of USB device arrivals or removals once the burst has settled, plus one follow-up read for the mouse to re-pair.
_Avoid_: hot-plug refresh, device-change refresh

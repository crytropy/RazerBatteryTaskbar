# Windows hardware verification

Phase 7 requires real Razer hardware testing because CI can validate code and packaging but cannot validate driver coexistence or physical hotplug behavior.

## Recommended test pass

Keep Razer Synapse running for the first pass.

1. Launch the latest Development Build.
2. Confirm each connected supported Razer device appears in the tray menu.
3. Choose **Copy diagnostics** and save the result.
4. Unplug a receiver or wired device. The tray should update without waiting for the 30-second poll.
5. Reconnect it. The device should return after the short hotplug settle delay.
6. Put Windows to sleep and resume. Device state should refresh shortly after resume.
7. Use **Refresh** to verify manual recovery still works.
8. Choose **Copy diagnostics** again after any failure.

Repeat without Synapse only if there is a read/access problem while Synapse is running.

## Important driver rule

Do not replace the Razer driver with Zadig/WinUSB merely to make this test pass. Phase 7 is specifically evaluating a transport that can coexist with the normal Windows/Razer stack.

## Diagnostic privacy

The copied diagnostics include application/runtime versions, transport state, VID/PID, product name, connection state, and battery/charging state. Device serial numbers are intentionally excluded.

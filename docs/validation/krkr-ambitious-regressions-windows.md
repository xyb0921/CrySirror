# AMBITIOUS MISSION image and save regression follow-up

2026-11-24. Built the Windows player in locked offline Release mode with
LTO disabled and 16 codegen units. Verified that the player SHA-256 differs
from the earlier binary that reproduced both failures. Both replays used
the same input tracks as the prior AM validation build, separate copies
of the test saves, Vulkan offscreen rendering and no audio device.

| Edition and action | Implementation | Behavior on the fresh Release player |
| --- | --- | --- |
| Main story, long Ctrl hold | Reserve the destination plus the peak decoded source during ordered composition. Add graph-owned encoded buffers separately and retain the 256 MiB composition limit. | Completed 15,600 frames, past the prior image composition limit failure, including the scripted Ctrl release. |
| After Episode 1, open Save during a transition and confirm slot 1 | Freeze the nearest crossfade endpoint in the destination root's local coordinates. Leave live transition state and the save format intact. | Completed 15,000 frames and reached the save page wait. The updated slot contains a 276 × 155 JPEG that decodes successfully. |

The snapshot test covers both transition halves, unchanged elapsed progress
and rejection of a crossfade that crosses an unrelated capture boundary.
The image budget test covers large ordered overlays without cumulative
charges for temporary decoded surfaces. The Windows test saves and captures
remain local.

## Backlog scene jump

The report from the main story stopped during Jump preview in
`Layer.clipAlphaRect`, which needs a resumable paint call. A Windows replay
of the same action on the prior player reproduced a separate rejection of an
`ltPsScreen` layer in `Layer.piledCopy`. The composed copy now lets the
paint call finish before capturing the scene. The copied subtree and each
crossfade endpoint keep the screen blend within their own composition.

After confirmation, the replay exposed a missing mutable AlphaMovie surface.
Canvas draws now use an immutable frame from the shared media service.
Stopping or removing the movie releases its own reference. Current layers and
frozen copies retain the picture until their final use.

The rebuilt Windows Release player completed 8,551 frames in the Vulkan
offscreen replay. The input opened Backlog, previewed an earlier line,
confirmed the jump and advanced once. The trace recorded the selected line
followed by the next one, and the renderer captured the following transition.
The replay used a separate save copy and disabled audio output. The
`clipAlphaRect` callback order was verified with a synthetic layer test.

## Expression overlays in slots 0004 and 0005

The Mode A AJPM decoder now uses the separate compressed alpha plane and
clears pixels outside the current frame rectangle. A local Windows Release
replay of slot 0004 captured the question-mark animation and the blue bubbles
from `emo15_2.amv`. At ticks 7936 and 7977, the previous player displayed an
opaque black rectangle around the blue bubbles, and the rebuilt player
preserved the face and classroom between them. These captures used 10 ms of
wall pacing per offscreen tick so the movie decoder could keep up with the
simulated clock. The codec tests cover partial alpha, a moving rectangle,
a blank frame, and a mask length that disagrees with the rectangle.

During slot 0005 restore, the portrait and its frame require several paint
callbacks and resource waits. Starting `clipAlphaRect` retains the last
complete desktop snapshot through the current paint pass. A synthetic test
waits for frame art after the first mask finishes and confirms that the
presentation hold lasts until both callbacks complete.

The real save replay captured the unfinished portrait composition at tick
7000 and recorded `presentation_pending: true`. The raw offscreen capture
bypasses the desktop presentation gate. At tick 7250 the hold had cleared and
only the portrait inside the cut-in frame was visible. These checks used local
save copies and Vulkan offscreen rendering.

## Portrait colors after expression redraws

Slot 0007 restores the park dialogue. The same scene from slot 0006 showed
progressive desaturation after repeated expression changes on the earlier
Windows player. The native trace paired a 299 × 294 drawing clip with
`Layer.modulate(0, -14, 0)` on a copy of the previous portrait. The shared
HSL effect now takes that clip and updates RGB inside it.

On the rebuilt Windows Release player, the same two captures at ticks 7500
and 8220 kept the median per-pixel RGB channel spread at 59 in an unchanged
region of the blue jacket. On the previous player those captures measured
50 and 36. A long Ctrl replay from slot 0005 reached the same park dialogue.
At tick 19200 the old jacket measured 11 and the new jacket still measured 59.
The direct load of slot 0007 and both scripted Ctrl holds and releases
completed. These replays used copied saves and the Vulkan offscreen renderer
with audio output disabled.

## CPU frame spikes during Ctrl fast-forward from slot 0007

2026-11-25. A Windows x86-64 Release comparison loaded the same slot and
held Ctrl for 1,800 virtual frames. The capture used Vulkan offscreen
rendering with audio output disabled. The CPU profiler measured the engine
frame, including the offscreen asset-completion quantum.

The TJS member table now allocates buckets on the first insertion. Empty
objects and reclaimed slots retain the logical bucket count and native hash
rebuild rules. The regression test covers lookup, removal and rebuild before
the first insert. The existing enumeration tests exercise collision order.

In the measured pair, the longest engine frame changed from 71.12 ms to
58.71 ms. The collector peak changed from 32.08 ms to 26.29 ms. P95 was
19.00 ms and 19.03 ms respectively. The capture after releasing Ctrl matched
exactly. Script callbacks and layer commits still accounted for the longest
frames in the updated replay.

## Sliced portrait redraws during Ctrl fast-forward from slot 0008

2026-11-25. Slot 0008 restores the two-person park scene, and slot 0009
restores a classroom scene. In the supplied park recording the picture stays
nearly unchanged for up to 3.37 seconds. These recordings cover different
scenes, so each slot is replayed independently for before/after verification.

The park script copies the same expression rectangle across body strips. In
one trace, six of eight source rectangles were entirely outside their strips.
The retained bitmap graph still expanded those strips before the subsequent
tiled Photoshop blend. The scripted Ctrl replay recorded a 752 ms virtual-time
gap between the first attempt and retry of one tile operation. That gap spans
multiple engine ticks. The script had stopped its voice buffers at skip entry.

The adapter now checks the source and destination bounds before retaining a
bitmap copy. Fully disjoint rectangles add no Canvas operation or CPU history.
Partial intersections retain their signed coordinates. The Release regression
test covers six empty strip copies followed by a partially intersecting strip.

In the first 6.4 seconds of the matching slot 0008 Ctrl input track, the
number of portrait color-adjustment calls increased from 9 to 18. The longest
retry gap for the tiled blend dropped from 752 to 384 ms of virtual time. The
slot 0009 trace had no calls to that blend during the same interval. Both
updated Windows Release replays received Ctrl-down and Ctrl-up, completed
9,050 ticks and exited successfully. All 24 KiriKiri library tests passed.
The replays used Vulkan offscreen rendering and copied saves with audio
output disabled. Interactive wall-clock pacing requires a windowed check.

## Reuse decoded inputs in tiled portrait redraws

2026-11-26. The park trace records Photoshop Soft Light operations after
expression copies, followed by a separate HSL adjustment. The image worker
reuses an immutable source across consecutive tiles. Compositions can reuse
prepared TLG pixels after verifying the current bytes with the job's storage
reader. The adapter skips blend tiles disjoint from the source or draw clip.
Partial intersections retain their original coordinates.

In the first 6.4 simulated seconds of the same slot 0008 Ctrl input, the
Soft Light retry count changed from 101 to 67 and the scene reached 19
color adjustments compared with 18. The longest retry gap changed from
384 to 400 simulated ms. Preparation of the slower portrait redraws still
dominates these waits. Slot 0009 did not use the same Soft Light loop in
this interval. The two updated Release replays completed 9,550 ticks with
presentation clear after Ctrl release. Both used separate saves, Vulkan
offscreen rendering and audio output disabled.

## Retained copy history during slot 0008 fast-forward

2026-11-26. A Windows Release diagnostic timed the CPU bitmap workers in the
same copy of slot 0008. Across the first 6.4 simulated seconds of the Ctrl
hold, a 1041 × 1152 composition grew from 45 to 261 input nodes in the
trace. The longest sampled worker task took 516 ms of wall time, including
372 ms in image decoding and 109 ms in bitmap blending. Worker queue
time for that task was 55 microseconds.

The shared compositor now limits a copied source recipe to the pixels
sampled by that copy. It releases previous destination writes fully covered
by a new full-opacity RGBA copy. Partially covered writes and blending order
remain in the graph. Source coordinates and output dimensions are preserved.

In paired 7,900-tick Release replays, the longest Soft Light retry gap
in the same 6.4-second input interval dropped from 416 to 128 simulated ms.
The scene completed 43 color adjustments in that interval, up from 19.
In a 9,550-tick repeat, the updated slot 0008 replay reached 45 adjustments
with a 112 ms maximum in the same interval. Both slots completed all 9,550
ticks and had released the presentation hold after Ctrl was released.
These timings use the offscreen clock. The Windows presenter needs its own
wall-clock check.

@file:OptIn(ExperimentalNativeApi::class, NativeRuntimeApi::class, ExperimentalStdlibApi::class)

package bench

import kotlin.experimental.ExperimentalNativeApi
import kotlin.native.Platform
import kotlin.native.runtime.NativeRuntimeApi
import kotlin.native.runtime.GC

actual fun platformName(): String =
    "native-${Platform.osFamily.name.lowercase()}-${Platform.cpuArchitecture.name.lowercase()}"

// The Kotlin/Native runtime only exposes the last collection; its epoch counts collections since start.
actual fun gcCount(): Long = GC.lastGCInfo?.let { it.epoch + 1 } ?: 0L

private var observedEpoch = -1L
private var observedCycleNanos = 0L
private var observedPauseNanos = 0L

actual fun gcTimeMillis(): Long = observedCycleNanos / 1_000_000

actual fun gcPauseMillis(): Long = observedPauseNanos / 1_000_000

actual fun observeGc() {
    val info = GC.lastGCInfo ?: return
    if (info.epoch == observedEpoch) return
    observedEpoch = info.epoch
    val end = info.endTimeNs ?: return
    observedCycleNanos += end - info.startTimeNs
    val firstEnd = info.firstPauseEndTimeNs
    if (firstEnd != null) observedPauseNanos += firstEnd - info.firstPauseStartTimeNs!!
    val secondEnd = info.secondPauseEndTimeNs
    if (secondEnd != null) observedPauseNanos += secondEnd - info.secondPauseStartTimeNs!!
}

actual fun configureNativeGc(targetHeapMegabytes: Int) {
    GC.autotune = false
    GC.targetHeapBytes = targetHeapMegabytes * 1024L * 1024L
}

actual fun availableProcessors(): Int = Platform.getAvailableProcessors()

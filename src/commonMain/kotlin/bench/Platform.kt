package bench

/** Human-readable runtime identifier, e.g. `jvm-25-mac_os_x-aarch64` or `native-macosx-arm64`. */
expect fun platformName(): String

/** Number of garbage collections since start, or -1 when the runtime does not expose it. */
expect fun gcCount(): Long

/** Cumulated duration of garbage collection cycles in milliseconds since start, concurrent phases included. */
expect fun gcTimeMillis(): Long

/** Cumulated stop-the-world pause time in milliseconds since start. */
expect fun gcPauseMillis(): Long

/**
 * Called after every measured iteration. Kotlin/Native only exposes the last collection, so it is polled
 * here to accumulate cycle and pause times. Collections that complete within the same iteration as a later
 * one are missed, so native GC times are a lower bound. No-op on the JVM.
 */
expect fun observeGc()

/** Fixes the Kotlin/Native GC target heap size and disables its auto-tuning. No-op on the JVM. */
expect fun configureNativeGc(targetHeapMegabytes: Int)

/** Number of hardware threads available to the process. */
expect fun availableProcessors(): Int

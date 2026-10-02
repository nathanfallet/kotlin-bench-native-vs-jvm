package bench

import kotlin.time.TimeSource

/**
 * Runs [body] `warmup + iterations` times and returns the duration of every call in nanoseconds,
 * warm-up calls included, so the JIT warm-up curve stays visible.
 */
inline fun sampleDurations(warmup: Int, iterations: Int, body: (Int) -> Unit): LongArray {
    val samples = LongArray(warmup + iterations)
    for (i in samples.indices) {
        val mark = TimeSource.Monotonic.markNow()
        body(i)
        samples[i] = mark.elapsedNow().inWholeNanoseconds
        observeGc()
    }
    return samples
}

class Result(
    val workload: String,
    val unit: String,
    val warmup: Int,
    val samples: LongArray,
    val checksum: Long,
    val setupNanos: Long,
    val gcCountDuring: Long,
    val gcMillisDuring: Long,
    val gcPauseMillisDuring: Long,
    val extra: Map<String, String> = emptyMap(),
) {
    fun toJson(): String {
        val measured = samples.copyOfRange(warmup, samples.size)
        val sorted = measured.sortedArray()
        fun percentile(p: Double): Long = sorted[((sorted.size - 1) * p).toInt()]
        val first = samples.copyOfRange(0, minOf(50, samples.size))
        val fields = linkedMapOf(
            "target" to "\"${platformName()}\"",
            "workload" to "\"$workload\"",
            "unit" to "\"$unit\"",
            "cpus" to availableProcessors().toString(),
            "warmup" to warmup.toString(),
            "iterations" to measured.size.toString(),
            "setupNs" to setupNanos.toString(),
            "totalNs" to measured.sum().toString(),
            "meanNs" to (measured.sum() / measured.size).toString(),
            "p50Ns" to percentile(0.50).toString(),
            "p90Ns" to percentile(0.90).toString(),
            "p99Ns" to percentile(0.99).toString(),
            "maxNs" to sorted.last().toString(),
            "first50MeanNs" to (first.sum() / first.size).toString(),
            "gcCount" to gcCountDuring.toString(),
            "gcMillis" to gcMillisDuring.toString(),
            "gcPauseMillis" to gcPauseMillisDuring.toString(),
            "checksum" to "\"${checksum.toULong().toString(16)}\"",
        )
        // Mean duration per tenth of the run, warm-up included, to show JIT warm-up and drift over time.
        val window = maxOf(1, samples.size / 10)
        fields["timelineNs"] = (0 until samples.size / window).joinToString(",", "[", "]") { w ->
            (samples.copyOfRange(w * window, (w + 1) * window).sum() / window).toString()
        }
        for ((key, value) in extra) fields[key] = "\"$value\""
        return "RESULT {" + fields.entries.joinToString(",") { "\"${it.key}\":${it.value}" } + "}"
    }
}

/** Measures a workload between two GC snapshots, so collections triggered during setup are not counted. */
inline fun measureWorkload(
    workload: String,
    unit: String,
    warmup: Int,
    iterations: Int,
    setupNanos: Long,
    checksum: () -> Long,
    noinline extra: () -> Map<String, String> = { emptyMap() },
    body: (Int) -> Unit,
): Result {
    observeGc()
    val gcBefore = gcCount()
    val gcTimeBefore = gcTimeMillis()
    val gcPauseBefore = gcPauseMillis()
    val samples = sampleDurations(warmup, iterations, body)
    val gcTimeAfter = gcTimeMillis()
    val gcPauseAfter = gcPauseMillis()
    return Result(
        workload = workload,
        unit = unit,
        warmup = warmup,
        samples = samples,
        checksum = checksum(),
        setupNanos = setupNanos,
        gcCountDuring = gcCount() - gcBefore,
        gcMillisDuring = gcTimeAfter - gcTimeBefore,
        gcPauseMillisDuring = gcPauseAfter - gcPauseBefore,
        extra = extra(),
    )
}

package bench

import kotlin.math.abs
import kotlin.math.floor

/**
 * `java.util.Random`, re-implemented bit for bit so both targets consume the exact same random stream.
 * Minecraft relies on this generator (and its successors) for determinism.
 */
class JavaRandom(seed: Long) {
    private var state = (seed xor MULTIPLIER) and MASK

    fun next(bits: Int): Int {
        state = (state * MULTIPLIER + ADDEND) and MASK
        return (state ushr (48 - bits)).toInt()
    }

    fun nextInt(): Int = next(32)

    fun nextInt(bound: Int): Int {
        var r = next(31)
        val m = bound - 1
        if (bound and m == 0) return ((bound.toLong() * r.toLong()) shr 31).toInt()
        var u = r
        r = u % bound
        while (u - r + m < 0) {
            u = next(31)
            r = u % bound
        }
        return r
    }

    fun nextLong(): Long = (next(32).toLong() shl 32) + next(32)

    fun nextDouble(): Double = ((next(26).toLong() shl 27) + next(27)) * DOUBLE_UNIT

    fun nextFloat(): Float = next(24) / (1 shl 24).toFloat()

    fun nextBoolean(): Boolean = next(1) != 0

    private companion object {
        const val MULTIPLIER = 0x5DEECE66DL
        const val ADDEND = 0xBL
        const val MASK = (1L shl 48) - 1
        const val DOUBLE_UNIT = 1.0 / (1L shl 53)
    }
}

fun floorInt(value: Double): Int = floor(value).toInt()

/**
 * Deterministic atan2 approximation built from arithmetic only, so both targets agree to the last bit.
 * Vanilla uses its own `Mth.atan2` for the same reason instead of the platform `Math.atan2`.
 */
fun fastAtan2(y: Double, x: Double): Double {
    val ax = abs(x)
    val ay = abs(y)
    if (ax == 0.0 && ay == 0.0) return 0.0
    val a = if (ax < ay) ax / ay else ay / ax
    val s = a * a
    var r = ((-0.0464964749 * s + 0.15931422) * s - 0.327622764) * s * a + a
    if (ay > ax) r = 1.57079637 - r
    if (x < 0) r = 3.14159274 - r
    if (y < 0) r = -r
    return r
}

/** FNV-1a style mixing used by every checksum. */
fun mixHash(hash: Long, value: Long): Long = (hash xor value) * 0x100000001B3L

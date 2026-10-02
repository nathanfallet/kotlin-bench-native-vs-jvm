package bench

import kotlin.math.floor

interface NoiseSampler {
    fun noise(x: Double, y: Double, z: Double): Double
}

/**
 * Ken Perlin's improved noise, shaped like vanilla's `ImprovedNoise`: pure arithmetic, no allocation.
 * Straight port: the gradient tables are companion-object constants, as a Java `static final` array becomes.
 */
class ImprovedNoise(random: JavaRandom) : NoiseSampler {
    private val xo = random.nextDouble() * 256.0
    private val yo = random.nextDouble() * 256.0
    private val zo = random.nextDouble() * 256.0
    private val permutation = IntArray(256) { it }

    init {
        for (i in 0 until 256) {
            val j = random.nextInt(256 - i)
            val tmp = permutation[i]
            permutation[i] = permutation[i + j]
            permutation[i + j] = tmp
        }
    }

    override fun noise(x: Double, y: Double, z: Double): Double {
        val dx = x + xo
        val dy = y + yo
        val dz = z + zo
        val ix = floor(dx).toInt()
        val iy = floor(dy).toInt()
        val iz = floor(dz).toInt()
        val fx = dx - ix
        val fy = dy - iy
        val fz = dz - iz

        val a = p(ix) + iy
        val aa = p(a) + iz
        val ab = p(a + 1) + iz
        val b = p(ix + 1) + iy
        val ba = p(b) + iz
        val bb = p(b + 1) + iz

        val u = fade(fx)
        val v = fade(fy)
        val w = fade(fz)
        return lerp(
            w,
            lerp(
                v,
                lerp(u, grad(p(aa), fx, fy, fz), grad(p(ba), fx - 1, fy, fz)),
                lerp(u, grad(p(ab), fx, fy - 1, fz), grad(p(bb), fx - 1, fy - 1, fz)),
            ),
            lerp(
                v,
                lerp(u, grad(p(aa + 1), fx, fy, fz - 1), grad(p(ba + 1), fx - 1, fy, fz - 1)),
                lerp(u, grad(p(ab + 1), fx, fy - 1, fz - 1), grad(p(bb + 1), fx - 1, fy - 1, fz - 1)),
            ),
        )
    }

    private fun p(i: Int): Int = permutation[i and 255]

    private fun fade(t: Double): Double = t * t * t * (t * (t * 6.0 - 15.0) + 10.0)

    private fun lerp(t: Double, a: Double, b: Double): Double = a + t * (b - a)

    private fun grad(hash: Int, x: Double, y: Double, z: Double): Double {
        val h = hash and 15
        return GRADIENT_X[h] * x + GRADIENT_Y[h] * y + GRADIENT_Z[h] * z
    }

    internal companion object {
        val GRADIENT_X = doubleArrayOf(1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, -1.0, 0.0)
        val GRADIENT_Y = doubleArrayOf(1.0, 1.0, -1.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0)
        val GRADIENT_Z = doubleArrayOf(0.0, 0.0, 0.0, 0.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 0.0, 1.0, 0.0, -1.0)
    }
}

/**
 * Same algorithm as [ImprovedNoise], but the gradient tables are copied into instance fields. On Kotlin/Native
 * every read of a companion-object property goes through an initialisation check; this variant avoids it.
 */
class ImprovedNoiseHoisted(random: JavaRandom) : NoiseSampler {
    private val gradientX = ImprovedNoise.GRADIENT_X
    private val gradientY = ImprovedNoise.GRADIENT_Y
    private val gradientZ = ImprovedNoise.GRADIENT_Z
    private val xo = random.nextDouble() * 256.0
    private val yo = random.nextDouble() * 256.0
    private val zo = random.nextDouble() * 256.0
    private val permutation = IntArray(256) { it }

    init {
        for (i in 0 until 256) {
            val j = random.nextInt(256 - i)
            val tmp = permutation[i]
            permutation[i] = permutation[i + j]
            permutation[i + j] = tmp
        }
    }

    override fun noise(x: Double, y: Double, z: Double): Double {
        val dx = x + xo
        val dy = y + yo
        val dz = z + zo
        val ix = floor(dx).toInt()
        val iy = floor(dy).toInt()
        val iz = floor(dz).toInt()
        val fx = dx - ix
        val fy = dy - iy
        val fz = dz - iz

        val a = p(ix) + iy
        val aa = p(a) + iz
        val ab = p(a + 1) + iz
        val b = p(ix + 1) + iy
        val ba = p(b) + iz
        val bb = p(b + 1) + iz

        val u = fade(fx)
        val v = fade(fy)
        val w = fade(fz)
        return lerp(
            w,
            lerp(
                v,
                lerp(u, grad(p(aa), fx, fy, fz), grad(p(ba), fx - 1, fy, fz)),
                lerp(u, grad(p(ab), fx, fy - 1, fz), grad(p(bb), fx - 1, fy - 1, fz)),
            ),
            lerp(
                v,
                lerp(u, grad(p(aa + 1), fx, fy, fz - 1), grad(p(ba + 1), fx - 1, fy, fz - 1)),
                lerp(u, grad(p(ab + 1), fx, fy - 1, fz - 1), grad(p(bb + 1), fx - 1, fy - 1, fz - 1)),
            ),
        )
    }

    private fun p(i: Int): Int = permutation[i and 255]

    private fun fade(t: Double): Double = t * t * t * (t * (t * 6.0 - 15.0) + 10.0)

    private fun lerp(t: Double, a: Double, b: Double): Double = a + t * (b - a)

    private fun grad(hash: Int, x: Double, y: Double, z: Double): Double {
        val h = hash and 15
        return gradientX[h] * x + gradientY[h] * y + gradientZ[h] * z
    }
}

/** Several octaves of noise, each twice the frequency and half the amplitude of the previous one. */
class PerlinOctaves(random: JavaRandom, octaves: Int, hoisted: Boolean = false) {
    private val levels: Array<NoiseSampler> =
        Array(octaves) { if (hoisted) ImprovedNoiseHoisted(random) else ImprovedNoise(random) }

    fun sample(x: Double, y: Double, z: Double): Double {
        var frequency = 1.0
        var amplitude = 1.0
        var total = 0.0
        for (level in levels) {
            total += level.noise(x * frequency, y * frequency, z * frequency) * amplitude
            frequency *= 2.0
            amplitude *= 0.5
        }
        return total
    }
}

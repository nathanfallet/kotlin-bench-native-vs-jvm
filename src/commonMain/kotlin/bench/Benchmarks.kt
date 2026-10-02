package bench

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.runBlocking
import kotlin.time.TimeSource

/**
 * Main workload: a Minecraft-like server tick over a 24x24-chunk world with about 3,300 mobs, items,
 * arrows and 20 scripted players. One sample = one tick.
 */
object TickBenchmark {
    fun run(options: Options): Result {
        val start = TimeSource.Monotonic.markNow()
        val level = createLevel(options)
        val setup = start.elapsedNow().inWholeNanoseconds
        val warmup = options.warmup ?: 400
        val iterations = options.iterations ?: 2000
        var peakEntities = 0
        return measureWorkload(
            workload = "tick",
            unit = "tick",
            warmup = warmup,
            iterations = iterations,
            setupNanos = setup,
            checksum = { level.finalChecksum() },
            extra = {
                mapOf(
                    "chunks" to level.chunks.size.toString(),
                    "entitiesAtEnd" to level.entities.size.toString(),
                    "peakEntities" to peakEntities.toString(),
                ) + Level.PHASES.mapIndexed { i, name -> "phase.$name.meanNs" to (level.phaseNanos[i] / iterations).toString() }
            },
        ) { index ->
            if (index == warmup) level.phaseNanos.fill(0)
            level.tick()
            if (level.entities.size > peakEntities) peakEntities = level.entities.size
        }
    }

    fun createLevel(options: Options): Level {
        val level = Level(options.seed, size = 24)
        val generator = TerrainGenerator(options.seed)
        for (x in 0 until level.size) for (z in 0 until level.size) {
            level.chunks.put(Chunk.key(x, z), generator.generate(x, z))
        }
        fun target(base: Int) = (base * options.scale).toInt()
        level.populationTargets[EntityType.ZOMBIE] = target(1200)
        level.populationTargets[EntityType.COW] = target(1000)
        level.populationTargets[EntityType.VILLAGER] = target(700)
        level.populationTargets[EntityType.SKELETON] = target(400)
        repeat(20) {
            val pos = level.randomSurfacePosition(margin = 32)
            val player = ServerPlayer(level)
            player.setPos(pos.x + 0.5, pos.y.toDouble(), pos.z + 0.5)
            level.players.add(player)
        }
        level.tick() // spawns the initial population
        return level
    }
}

/**
 * Control workload: octave Perlin noise over a 32^3 grid. Pure floating-point arithmetic, zero allocation.
 * `hoisted` selects the variant whose lookup tables live in instance fields instead of a companion object.
 */
object NoiseBenchmark {
    fun run(options: Options, hoisted: Boolean): Result {
        val noise = PerlinOctaves(JavaRandom(options.seed), 6, hoisted)
        var checksum = 0L
        return measureWorkload(
            workload = if (hoisted) "noise-hoisted" else "noise",
            unit = "32^3 samples x 6 octaves",
            warmup = options.warmup ?: 30,
            iterations = options.iterations ?: 200,
            setupNanos = 0,
            checksum = { checksum },
        ) { iteration ->
            var sum = 0.0
            for (x in 0 until 32) for (y in 0 until 32) for (z in 0 until 32) {
                sum += noise.sample((x + iteration * 32) / 64.0, y / 64.0, z / 64.0)
            }
            checksum = mixHash(checksum, sum.toRawBits())
        }
    }
}

/**
 * Idiomatic Kotlin collections with boxed keys and values: `HashMap<Long, _>` churn, `ArrayList<Int>`,
 * sort, sequences and `groupBy`. This is what naively ported code looks like without fastutil.
 */
object CollectionsBenchmark {
    private class Payload(val a: Int, val b: Int)

    fun run(options: Options): Result {
        val random = JavaRandom(options.seed)
        val map = HashMap<Long, Payload>()
        var checksum = 0L
        return measureWorkload(
            workload = "collections",
            unit = "iteration",
            warmup = options.warmup ?: 30,
            iterations = options.iterations ?: 300,
            setupNanos = 0,
            checksum = { checksum },
        ) {
            repeat(50_000) {
                val key = random.nextInt(200_000).toLong() * 31L
                if (map.remove(key) == null) map[key] = Payload(key.toInt(), random.nextInt(1000))
            }
            var sum = map.values.asSequence().filter { it.b % 3 == 0 }.map { it.a.toLong() * it.b }.sum()
            val list = ArrayList<Int>()
            repeat(20_000) { list.add(random.nextInt(1_000_000)) }
            list.sort()
            sum += list[list.size / 2]
            val groups = list.groupBy { it % 16 }
            sum += groups.values.sumOf { it.size.toLong() * it.first() }
            checksum = mixHash(checksum, sum)
        }
    }
}

/**
 * Chunk generation in batches of 64 chunks: noise-heavy with allocation from feature placement.
 * `--threads 1` generates sequentially; otherwise the batch is spread over `Dispatchers.Default`.
 */
object WorldgenBenchmark {
    fun run(options: Options): Result {
        val generator = TerrainGenerator(options.seed)
        val threads = options.threads ?: availableProcessors()
        var checksum = 0L
        var batch = 0
        return measureWorkload(
            workload = if (threads == 1) "worldgen-1-thread" else "worldgen-$threads-threads",
            unit = "batch of 64 chunks",
            warmup = options.warmup ?: 5,
            iterations = options.iterations ?: 40,
            setupNanos = 0,
            checksum = { checksum },
        ) {
            val base = batch++ * 8
            val hashes: List<Long> = if (threads == 1) {
                (0 until 64).map { generator.generate(base + it / 8, it % 8).contentHash() }
            } else {
                runBlocking {
                    val dispatcher = Dispatchers.Default.limitedParallelism(threads)
                    (0 until 64).map { async(dispatcher) { generator.generate(base + it / 8, it % 8).contentHash() } }.awaitAll()
                }
            }
            for (hash in hashes) checksum = mixHash(checksum, hash)
        }
    }
}

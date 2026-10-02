package bench

/**
 * Micro-benchmarks, each isolating one mechanism that the tick uses heavily. One iteration performs
 * [OPERATIONS] operations; the report divides by it to give nanoseconds per operation.
 */
object MicroBenchmark {
    const val OPERATIONS = 1_000_000

    class Case(val description: String, val run: (seed: Long) -> () -> Long)

    val cases: Map<String, Case> = linkedMapOf(
        "alloc-temporary" to Case("allocate a small data class that dies immediately (BlockPos.offset)") { seed ->
            var pos = BlockPos(seed.toInt() and 1023, 64, 0)
            ;{
                var acc = 0L
                for (i in 0 until OPERATIONS) {
                    val next = pos.offset(1, 0, -1)
                    acc += next.x + next.z
                    if (i and 1023 == 0) pos = next
                }
                acc
            }
        },
        "alloc-retained" to Case("allocate a small data class and keep it alive for a while (ring of 64k slots)") { seed ->
            val ring = arrayOfNulls<BlockPos>(65536)
            var x = seed.toInt() and 1023
            ;{
                var acc = 0L
                for (i in 0 until OPERATIONS) {
                    val pos = BlockPos(x++, 64, i)
                    ring[i and 65535] = pos
                    acc += ring[(i + 1) and 65535]?.y ?: 0
                }
                acc
            }
        },
        "value-class-temporary" to Case("same as alloc-temporary with a @JvmInline value class packed in a Long") { seed ->
            var pos = packedPos(seed.toInt() and 1023, 64, 0)
            ;{
                var acc = 0L
                for (i in 0 until OPERATIONS) {
                    val next = pos.offset(1, 0, -1)
                    acc += next.x + next.z
                    if (i and 1023 == 0) pos = next
                }
                acc
            }
        },
        "virtual-call-megamorphic" to Case("call an overridden getter on 7 different block classes") { seed ->
            val random = JavaRandom(seed)
            val blocks = Array(4096) { Blocks.byId(random.nextInt(Blocks.WHEAT.last().id + 1)) }
            ;{
                var acc = 0L
                for (i in 0 until OPERATIONS) if (blocks[i and 4095].isSolid) acc++
                acc
            }
        },
        "virtual-call-monomorphic" to Case("same call when every element has the same class") { _ ->
            val blocks = Array(4096) { Blocks.GRASS }
            ;{
                var acc = 0L
                for (i in 0 until OPERATIONS) if (blocks[i and 4095].isSolid) acc++
                acc
            }
        },
        "companion-access-in-call" to Case("call a small method that reads a table from a companion object") { seed ->
            val random = JavaRandom(seed)
            val indices = IntArray(4096) { random.nextInt(16) }
            val reader: TableReader = CompanionTableReader()
            ;{
                var acc = 0.0
                for (i in 0 until OPERATIONS) acc += reader.read(indices[i and 4095])
                acc.toRawBits()
            }
        },
        "field-access-in-call" to Case("same method reading the table from an instance field") { seed ->
            val random = JavaRandom(seed)
            val indices = IntArray(4096) { random.nextInt(16) }
            val reader: TableReader = FieldTableReader()
            ;{
                var acc = 0.0
                for (i in 0 until OPERATIONS) acc += reader.read(indices[i and 4095])
                acc.toRawBits()
            }
        },
        "list-iterator" to Case("for-in loop over an ArrayList (allocates an iterator per loop)") { seed ->
            val lists = List(1000) { i -> ArrayList<Int>().apply { repeat(8) { add((seed.toInt() + i + it) and 1023) } } }
            ;{
                var acc = 0L
                for (i in 0 until OPERATIONS / 8) for (value in lists[i % 1000]) acc += value
                acc
            }
        },
        "list-indexed" to Case("same sum with an indexed loop, no iterator") { seed ->
            val lists = List(1000) { i -> ArrayList<Int>().apply { repeat(8) { add((seed.toInt() + i + it) and 1023) } } }
            ;{
                var acc = 0L
                for (i in 0 until OPERATIONS / 8) {
                    val list = lists[i % 1000]
                    for (j in 0 until list.size) acc += list[j]
                }
                acc
            }
        },
        "lambda-pipeline" to Case("filter { } then minByOrNull { } on an 8-element list, like the goal selector") { seed ->
            val lists = List(1000) { i -> List(8) { (seed.toInt() + i * 7 + it * 13) and 1023 } }
            ;{
                var acc = 0L
                for (i in 0 until OPERATIONS / 8) {
                    acc += lists[i % 1000].filter { it and 1 == 0 }.minByOrNull { it and 63 } ?: 0
                }
                acc
            }
        },
        "boxed-hashset-int" to Case("HashSet<Int> add + contains with ids above the boxing cache, like player tracking") { seed ->
            val base = (seed and 0xFFFF).toInt() + 100_000
            ;{
                val set = HashSet<Int>()
                var acc = 0L
                for (i in 0 until OPERATIONS / 2) {
                    set.add(base + (i and 4095))
                    if ((base + ((i * 7) and 8191)) in set) acc++
                }
                acc
            }
        },
        "primitive-long-map" to Case("LongObjectMap get, the fastutil-style map used for chunks") { seed ->
            val map = LongObjectMap<String>()
            repeat(4096) { map.put(it.toLong() * 31 + seed, "v") }
            ;{
                var acc = 0L
                for (i in 0 until OPERATIONS) if (map[(i and 8191).toLong() * 31 + seed] != null) acc++
                acc
            }
        },
        "boxed-long-hashmap" to Case("HashMap<Long, _> get with the same keys, boxing every lookup key") { seed ->
            val map = HashMap<Long, String>()
            repeat(4096) { map[it.toLong() * 31 + seed] = "v" }
            ;{
                var acc = 0L
                for (i in 0 until OPERATIONS) if (map[(i and 8191).toLong() * 31 + seed] != null) acc++
                acc
            }
        },
        "array-arithmetic" to Case("integer and double arithmetic over primitive arrays, no allocation, no call") { seed ->
            val a = IntArray(4096) { (it * 31 + seed.toInt()) and 1023 }
            val b = DoubleArray(4096) { it * 0.5 }
            ;{
                var acc = 0.0
                for (i in 0 until OPERATIONS) {
                    val j = i and 4095
                    acc += a[j] * b[j] - (a[j] shr 3)
                }
                acc.toRawBits()
            }
        },
    )

    fun run(options: Options): Result {
        val name = requireNotNull(options.case) { "micro needs --case, one of: ${cases.keys.joinToString()}" }
        val case = requireNotNull(cases[name]) { "unknown case '$name', expected one of: ${cases.keys.joinToString()}" }
        val body = case.run(options.seed)
        var checksum = 0L
        return measureWorkload(
            workload = "micro:$name",
            unit = "$OPERATIONS operations",
            warmup = options.warmup ?: 50,
            iterations = options.iterations ?: 300,
            setupNanos = 0,
            checksum = { checksum },
            extra = { mapOf("description" to case.description, "operations" to OPERATIONS.toString()) },
        ) {
            checksum = mixHash(checksum, body())
        }
    }
}

/** A block position packed into one Long, the representation vanilla uses in its hot paths (`BlockPos.asLong`). */
@kotlin.jvm.JvmInline
value class PackedPos(val packed: Long) {
    val x: Int get() = (packed shr 38).toInt()
    val y: Int get() = ((packed shl 52) shr 52).toInt()
    val z: Int get() = ((packed shl 26) shr 38).toInt()

    fun offset(dx: Int, dy: Int, dz: Int) = packedPos(x + dx, y + dy, z + dz)
}

// Top-level rather than in a companion object, so the measurement is not polluted by object access checks.
fun packedPos(x: Int, y: Int, z: Int) =
    PackedPos(((x.toLong() and 0x3FFFFFF) shl 38) or ((z.toLong() and 0x3FFFFFF) shl 12) or (y.toLong() and 0xFFF))

/** Called through an interface so neither compiler can inline the read into the caller's loop. */
interface TableReader {
    fun read(index: Int): Double
}

class CompanionTableReader : TableReader {
    override fun read(index: Int): Double = TABLE[index]

    private companion object {
        val TABLE = DoubleArray(16) { it * 0.25 - 2.0 }
    }
}

class FieldTableReader : TableReader {
    private val table = DoubleArray(16) { it * 0.25 - 2.0 }

    override fun read(index: Int): Double = table[index]
}

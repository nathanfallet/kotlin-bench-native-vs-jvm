package bench

class Options(
    val workload: String,
    val warmup: Int?,
    val iterations: Int?,
    val seed: Long,
    val threads: Int?,
    val scale: Double,
    val nativeHeapMegabytes: Int?,
    val case: String?,
) {
    companion object {
        fun parse(args: Array<String>): Options {
            require(args.isNotEmpty()) { USAGE }
            val flags = HashMap<String, String>()
            var i = 1
            while (i < args.size) {
                val name = args[i].removePrefix("--")
                require(i + 1 < args.size) { "missing value for --$name\n$USAGE" }
                flags[name] = args[i + 1]
                i += 2
            }
            return Options(
                workload = args[0],
                warmup = flags["warmup"]?.toInt(),
                iterations = flags["iterations"]?.toInt(),
                seed = flags["seed"]?.toLong() ?: 20260930L,
                threads = flags["threads"]?.toInt(),
                scale = flags["scale"]?.toDouble() ?: 1.0,
                nativeHeapMegabytes = flags["native-heap-mb"]?.toInt(),
                case = flags["case"],
            )
        }

        private const val USAGE =
            "usage: bench <startup|tick|noise|noise-hoisted|collections|worldgen|micro> " +
                "[--warmup N] [--iterations N] [--seed N] [--threads N] [--scale X] [--native-heap-mb N] [--case NAME]"
    }
}

fun main(args: Array<String>) {
    val options = Options.parse(args)
    options.nativeHeapMegabytes?.let { configureNativeGc(it) }
    val result = when (options.workload) {
        "startup" -> {
            // Smallest possible run: measures process start-up and runtime initialisation only.
            println("""RESULT {"target":"${platformName()}","workload":"startup"}""")
            return
        }
        "tick" -> TickBenchmark.run(options)
        "noise" -> NoiseBenchmark.run(options, hoisted = false)
        "noise-hoisted" -> NoiseBenchmark.run(options, hoisted = true)
        "collections" -> CollectionsBenchmark.run(options)
        "worldgen" -> WorldgenBenchmark.run(options)
        "micro" -> MicroBenchmark.run(options)
        else -> error("unknown workload '${options.workload}'")
    }
    println(result.toJson())
}

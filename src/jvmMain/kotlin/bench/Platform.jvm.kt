package bench

import java.lang.management.ManagementFactory

actual fun platformName(): String {
    val os = System.getProperty("os.name").lowercase().replace(' ', '_')
    return "jvm-${System.getProperty("java.version")}-$os-${System.getProperty("os.arch")}"
}

actual fun gcCount(): Long =
    ManagementFactory.getGarbageCollectorMXBeans().sumOf { maxOf(0L, it.collectionCount) }

actual fun gcTimeMillis(): Long =
    ManagementFactory.getGarbageCollectorMXBeans().sumOf { maxOf(0L, it.collectionTime) }

// G1 reports its concurrent cycles in a separate "G1 Concurrent GC" bean; the other beans are pauses.
actual fun gcPauseMillis(): Long =
    ManagementFactory.getGarbageCollectorMXBeans()
        .filter { "Concurrent" !in it.name && "Cycles" !in it.name }
        .sumOf { maxOf(0L, it.collectionTime) }

actual fun observeGc() {}

actual fun configureNativeGc(targetHeapMegabytes: Int) {}

actual fun availableProcessors(): Int = Runtime.getRuntime().availableProcessors()

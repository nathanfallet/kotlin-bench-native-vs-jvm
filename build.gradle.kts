import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    kotlin("multiplatform") version "2.4.20"
}

kotlin {
    jvm {
        compilerOptions {
            jvmTarget.set(JvmTarget.JVM_21)
        }
    }

    // Every native target is built from the same common code. Only the host-compatible ones can run locally:
    // macosArm64 on Apple Silicon, linuxArm64 in an arm64 Docker container.
    listOf(macosArm64(), linuxArm64(), linuxX64(), mingwX64()).forEach { target ->
        target.binaries.executable {
            entryPoint = "bench.main"
            baseName = "bench"
        }
    }

    // Kotlin/Native compiles linuxArm64 for cortex-a57 and linuxX64 for baseline x86-64 by default.
    // `-PlinuxArm64Cpu=neoverse-n1` (or `-PlinuxX64Cpu=x86-64-v3`) targets a modern server CPU instead.
    val overrides = listOfNotNull(
        (findProperty("linuxArm64Cpu") as String?)?.let { "targetCpu.linux_arm64=$it" },
        (findProperty("linuxX64Cpu") as String?)?.let { "targetCpu.linux_x64=$it" },
    )
    if (overrides.isNotEmpty()) {
        targets.withType<org.jetbrains.kotlin.gradle.plugin.mpp.KotlinNativeTarget>().configureEach {
            binaries.all { freeCompilerArgs += "-Xoverride-konan-properties=${overrides.joinToString(";")}" }
        }
    }

    sourceSets {
        commonMain.dependencies {
            implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.11.0")
        }
    }
}

// Self-contained jar so the JVM side is launched exactly like the native binary: one process, no Gradle.
val jvmFatJar by tasks.registering(Jar::class) {
    group = "build"
    description = "Assembles a runnable jar of the JVM target with its dependencies."
    archiveBaseName.set("bench-jvm")
    archiveClassifier.set("all")
    manifest {
        attributes["Main-Class"] = "bench.MainKt"
    }
    val jvmJar = tasks.named<Jar>("jvmJar")
    dependsOn(jvmJar)
    from(jvmJar.map { zipTree(it.archiveFile) })
    from(configurations.named("jvmRuntimeClasspath").map { classpath -> classpath.map { zipTree(it) } })
    duplicatesStrategy = DuplicatesStrategy.EXCLUDE
    exclude("META-INF/*.SF", "META-INF/*.DSA", "META-INF/*.RSA")
}

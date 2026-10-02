# Linux runner: JDK 25 for the JVM side, GNU time for peak RSS, Python for scripts/run.py.
# The native side is the linuxArm64 (or linuxX64) binary built on the host and mounted into the container.
FROM eclipse-temurin:25-jdk

RUN apt-get update \
    && apt-get install -y --no-install-recommends python3 time \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /bench

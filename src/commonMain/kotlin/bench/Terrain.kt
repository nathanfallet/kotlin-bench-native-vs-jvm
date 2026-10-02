package bench

/** A 16 x HEIGHT x 16 column of block ids, stored flat like a vanilla palette-less section. */
class Chunk(val x: Int, val z: Int) {
    val blocks = ShortArray(16 * 16 * HEIGHT)

    fun get(localX: Int, y: Int, localZ: Int): Int = blocks[index(localX, y, localZ)].toInt()

    fun set(localX: Int, y: Int, localZ: Int, id: Int) {
        blocks[index(localX, y, localZ)] = id.toShort()
    }

    fun surfaceY(localX: Int, localZ: Int): Int {
        for (y in HEIGHT - 1 downTo 1) {
            if (Blocks.byId(get(localX, y, localZ)).isSolid) return y
        }
        return 0
    }

    fun contentHash(): Long {
        var hash = mixHash(0L, (x.toLong() shl 32) or (z.toLong() and 0xFFFFFFFFL))
        for (i in blocks.indices step 7) hash = mixHash(hash, blocks[i].toLong())
        return hash
    }

    companion object {
        const val HEIGHT = 128
        const val SEA_LEVEL = 56

        fun key(chunkX: Int, chunkZ: Int): Long = (chunkX.toLong() and 0xFFFFFFFFL) or (chunkZ.toLong() shl 32)

        private fun index(localX: Int, y: Int, localZ: Int) = (y shl 8) or (localZ shl 4) or localX
    }
}

/**
 * Terrain with height noise, 3D cave noise, trees and wheat patches. Heavy on floating-point work,
 * with vanilla-like allocation of positions while placing features. Safe to share between threads.
 */
class TerrainGenerator(private val seed: Long) {
    private val heightNoise: PerlinOctaves
    private val detailNoise: PerlinOctaves
    private val caveNoise: PerlinOctaves

    init {
        val random = JavaRandom(seed)
        heightNoise = PerlinOctaves(random, 4)
        detailNoise = PerlinOctaves(random, 2)
        caveNoise = PerlinOctaves(random, 3)
    }

    fun generate(chunkX: Int, chunkZ: Int): Chunk {
        val chunk = Chunk(chunkX, chunkZ)
        val random = JavaRandom(seed * 341873128712L + chunkX * 132897987541L + chunkZ)
        for (localX in 0 until 16) for (localZ in 0 until 16) {
            val worldX = chunkX * 16 + localX
            val worldZ = chunkZ * 16 + localZ
            val height = (
                Chunk.SEA_LEVEL +
                    heightNoise.sample(worldX / 160.0, 0.0, worldZ / 160.0) * 28.0 +
                    detailNoise.sample(worldX / 24.0, 10.0, worldZ / 24.0) * 4.0
                ).toInt().coerceIn(8, Chunk.HEIGHT - 20)
            for (y in 0 until Chunk.HEIGHT) {
                var block = when {
                    y == 0 -> Blocks.STONE
                    y < height - 3 -> Blocks.STONE
                    y < height -> Blocks.DIRT
                    y == height -> if (height > Chunk.SEA_LEVEL) Blocks.GRASS else Blocks.SAND
                    y <= Chunk.SEA_LEVEL -> Blocks.WATER
                    else -> Blocks.AIR
                }
                if (y in 1 until height - 1 && caveNoise.sample(worldX / 40.0, y / 20.0, worldZ / 40.0) > 0.42) {
                    block = Blocks.AIR
                }
                chunk.set(localX, y, localZ, block.id)
            }
        }
        placeTrees(chunk, random)
        placeWheat(chunk, random)
        return chunk
    }

    private fun placeTrees(chunk: Chunk, random: JavaRandom) {
        repeat(random.nextInt(5)) {
            val x = 2 + random.nextInt(12)
            val z = 2 + random.nextInt(12)
            val ground = chunk.surfaceY(x, z)
            if (chunk.get(x, ground, z) != Blocks.GRASS.id || ground > Chunk.HEIGHT - 12) return@repeat
            val trunkHeight = 4 + random.nextInt(3)
            val leaves = ArrayList<BlockPos>()
            val top = BlockPos(x, ground + trunkHeight, z)
            for (dx in -2..2) for (dy in -2..1) for (dz in -2..2) {
                val candidate = top.offset(dx, dy, dz)
                if (candidate.distSqr(top) <= 5 + random.nextInt(2)) leaves.add(candidate)
            }
            for (pos in leaves) {
                if (chunk.get(pos.x, pos.y, pos.z) == Blocks.AIR.id) chunk.set(pos.x, pos.y, pos.z, Blocks.LEAVES.id)
            }
            for (y in ground + 1..ground + trunkHeight) chunk.set(x, y, z, Blocks.LOG.id)
        }
    }

    private fun placeWheat(chunk: Chunk, random: JavaRandom) {
        if (random.nextInt(3) != 0) return
        val startX = random.nextInt(10)
        val startZ = random.nextInt(10)
        for (x in startX until startX + 6) for (z in startZ until startZ + 6) {
            val ground = chunk.surfaceY(x, z)
            if (chunk.get(x, ground, z) == Blocks.GRASS.id && chunk.get(x, ground + 1, z) == Blocks.AIR.id) {
                chunk.set(x, ground, z, Blocks.DIRT.id)
                chunk.set(x, ground + 1, z, Blocks.WHEAT[random.nextInt(CropBlock.MAX_STAGE + 1)].id)
            }
        }
    }
}

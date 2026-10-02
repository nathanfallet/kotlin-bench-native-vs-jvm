package bench

import kotlin.time.TimeSource

class ScheduledTick(val pos: BlockPos, val block: Block, val time: Long, val order: Long)

/** Binary min-heap of scheduled block ticks, ordered by due time then insertion order, like vanilla's `LevelTicks`. */
class TickQueue {
    private var heap = arrayOfNulls<ScheduledTick>(256)
    var size = 0
        private set

    fun add(tick: ScheduledTick) {
        if (size == heap.size) heap = heap.copyOf(size * 2)
        var i = size++
        heap[i] = tick
        while (i > 0) {
            val parent = (i - 1) / 2
            if (!before(heap[i]!!, heap[parent]!!)) break
            swap(i, parent)
            i = parent
        }
    }

    fun peek(): ScheduledTick? = if (size == 0) null else heap[0]

    fun poll(): ScheduledTick {
        val top = heap[0]!!
        heap[0] = heap[--size]
        heap[size] = null
        var i = 0
        while (true) {
            val left = 2 * i + 1
            if (left >= size) break
            val right = left + 1
            val child = if (right < size && before(heap[right]!!, heap[left]!!)) right else left
            if (!before(heap[child]!!, heap[i]!!)) break
            swap(i, child)
            i = child
        }
        return top
    }

    private fun before(a: ScheduledTick, b: ScheduledTick) = a.time < b.time || (a.time == b.time && a.order < b.order)

    private fun swap(a: Int, b: Int) {
        val tmp = heap[a]
        heap[a] = heap[b]
        heap[b] = tmp
    }
}

class EntitySection {
    val entities = ArrayList<Entity>()
}

/**
 * A square world of `size x size` chunks with walls at the border. One call to [tick] runs the phases of
 * vanilla's `ServerLevel.tick`: scheduled ticks, random ticks, entities, then player tracking and packets.
 */
class Level(val seed: Long, val size: Int) {
    val random = JavaRandom(seed xor 0x2545F4914F6CDD1DL)
    val chunks = LongObjectMap<Chunk>(size * size)
    private val sections = LongObjectMap<EntitySection>(4096)
    val entities = ArrayList<Entity>()
    private val pendingEntities = ArrayList<Entity>()
    val players = ArrayList<ServerPlayer>()
    val changedBlocks = ArrayList<BlockPos>()
    private val scheduledTicks = TickQueue()
    private var tickOrder = 0L
    var gameTime = 0L
        private set
    var nextEntityId = 1
    var checksum = 0L
    val populationTargets = HashMap<EntityType, Int>()
    private val population = HashMap<EntityType, Int>()
    val widthInBlocks get() = size * 16

    fun getBlock(pos: BlockPos): Block {
        if (pos.y < 0 || pos.y >= Chunk.HEIGHT) return Blocks.AIR
        val chunk = chunks[Chunk.key(pos.x shr 4, pos.z shr 4)] ?: return Blocks.STONE
        return Blocks.byId(chunk.get(pos.x and 15, pos.y, pos.z and 15))
    }

    fun setBlock(pos: BlockPos, block: Block) {
        if (pos.y <= 0 || pos.y >= Chunk.HEIGHT) return
        val chunk = chunks[Chunk.key(pos.x shr 4, pos.z shr 4)] ?: return
        if (chunk.get(pos.x and 15, pos.y, pos.z and 15) == block.id) return
        chunk.set(pos.x and 15, pos.y, pos.z and 15, block.id)
        changedBlocks.add(pos)
        block.onPlace(this, pos)
        for (direction in Direction.entries) {
            val neighbor = pos.relative(direction)
            getBlock(neighbor).neighborChanged(this, neighbor, pos)
        }
    }

    fun surfaceY(x: Int, z: Int): Int {
        val chunk = chunks[Chunk.key(x shr 4, z shr 4)] ?: return Chunk.SEA_LEVEL
        return chunk.surfaceY(x and 15, z and 15)
    }

    fun scheduleTick(pos: BlockPos, block: Block, delay: Int) {
        scheduledTicks.add(ScheduledTick(pos, block, gameTime + delay, tickOrder++))
    }

    fun addEntity(entity: Entity) {
        pendingEntities.add(entity)
        population[entity.type] = (population[entity.type] ?: 0) + 1
    }

    fun onRemoved(entity: Entity) {
        sections[entity.sectionKey]?.entities?.remove(entity)
        population[entity.type] = (population[entity.type] ?: 1) - 1
    }

    fun updateSection(entity: Entity) {
        val key = sectionKey(floorInt(entity.x) shr 4, floorInt(entity.y) shr 4, floorInt(entity.z) shr 4)
        if (key == entity.sectionKey) return
        if (entity.sectionKey != NO_SECTION) sections[entity.sectionKey]?.entities?.remove(entity)
        sections.getOrPut(key) { EntitySection() }.entities.add(entity)
        entity.sectionKey = key
    }

    /** Entities whose box intersects [box], found through the section map like `EntitySectionStorage`. */
    inline fun getEntities(except: Entity?, box: AABB, predicate: (Entity) -> Boolean): ArrayList<Entity> {
        val result = ArrayList<Entity>()
        val minX = floorInt(box.minX - 2.0) shr 4
        val maxX = floorInt(box.maxX + 2.0) shr 4
        val minY = floorInt(box.minY - 4.0) shr 4
        val maxY = floorInt(box.maxY + 2.0) shr 4
        val minZ = floorInt(box.minZ - 2.0) shr 4
        val maxZ = floorInt(box.maxZ + 2.0) shr 4
        for (sx in minX..maxX) for (sz in minZ..maxZ) for (sy in minY..maxY) {
            val section = sectionAt(sx, sy, sz) ?: continue
            for (entity in section.entities) {
                if (entity !== except && !entity.removed && entity.bb.intersects(box) && predicate(entity)) {
                    result.add(entity)
                }
            }
        }
        return result
    }

    fun sectionAt(x: Int, y: Int, z: Int): EntitySection? = sections[sectionKey(x, y, z)]

    /** Solid block boxes overlapping [box], one allocated position and box per cell, like `getBlockCollisions`. */
    fun getBlockCollisions(box: AABB): ArrayList<AABB> {
        val result = ArrayList<AABB>()
        for (x in floorInt(box.minX)..floorInt(box.maxX)) {
            for (y in floorInt(box.minY)..floorInt(box.maxY)) {
                for (z in floorInt(box.minZ)..floorInt(box.maxZ)) {
                    val pos = BlockPos(x, y, z)
                    if (getBlock(pos).isSolid) result.add(AABB.ofBlock(pos.x, pos.y, pos.z))
                }
            }
        }
        return result
    }

    fun randomSurfacePosition(margin: Int = 4): BlockPos {
        val x = margin + random.nextInt(widthInBlocks - 2 * margin)
        val z = margin + random.nextInt(widthInBlocks - 2 * margin)
        return BlockPos(x, surfaceY(x, z) + 1, z)
    }

    /** Cumulated time per tick phase, indexed like [PHASES]. Reset by the benchmark after warm-up. */
    val phaseNanos = LongArray(PHASES.size)

    private inline fun phase(index: Int, body: () -> Unit) {
        val mark = TimeSource.Monotonic.markNow()
        body()
        phaseNanos[index] += mark.elapsedNow().inWholeNanoseconds
    }

    fun tick() {
        gameTime++
        phase(0) { runScheduledTicks() }
        phase(1) { randomTicks() }
        phase(2) { tickEntities() }
        phase(3) { for (player in players) player.tick() }
        phase(4) {
            for (entity in entities) entity.prepareTrackingDelta()
            for (player in players) checksum = mixHash(checksum, player.connection.sendChanges(this))
            for (entity in entities) entity.commitTrackingDelta()
            changedBlocks.clear()
        }
        phase(5) { respawn() }
    }

    private fun runScheduledTicks() {
        var budget = 4096
        while (budget-- > 0) {
            val next = scheduledTicks.peek() ?: break
            if (next.time > gameTime) break
            scheduledTicks.poll()
            if (getBlock(next.pos) === next.block) next.block.tick(this, next.pos, random)
        }
    }

    private fun randomTicks() {
        chunks.forEachValue { chunk ->
            for (sectionY in 0 until Chunk.HEIGHT / 16) {
                repeat(RANDOM_TICK_SPEED) {
                    val localX = random.nextInt(16)
                    val y = sectionY * 16 + random.nextInt(16)
                    val localZ = random.nextInt(16)
                    val block = Blocks.byId(chunk.get(localX, y, localZ))
                    if (block.isRandomlyTicking) {
                        block.randomTick(this, BlockPos(chunk.x * 16 + localX, y, chunk.z * 16 + localZ), random)
                    }
                }
            }
        }
    }

    private fun tickEntities() {
        for (i in entities.indices) {
            val entity = entities[i]
            if (!entity.removed) entity.tick()
        }
        entities.removeAll { it.removed }
        for (entity in pendingEntities) {
            if (!entity.removed) entities.add(entity)
        }
        pendingEntities.clear()
    }

    private fun respawn() {
        for ((type, target) in populationTargets.entries.sortedBy { it.key.ordinal }) {
            var missing = target - (population[type] ?: 0)
            while (missing-- > 0) {
                val pos = randomSurfacePosition()
                addEntity(type.create(this).apply { setPos(pos.x + 0.5, pos.y.toDouble(), pos.z + 0.5) })
            }
        }
    }

    fun finalChecksum(): Long {
        var hash = checksum
        for (entity in entities) {
            hash = mixHash(hash, entity.id.toLong())
            hash = mixHash(hash, entity.x.toRawBits())
            hash = mixHash(hash, entity.y.toRawBits())
            hash = mixHash(hash, entity.z.toRawBits())
        }
        chunks.forEachValue { hash = mixHash(hash, it.contentHash()) }
        return hash
    }

    companion object {
        val PHASES = listOf("scheduledTicks", "randomTicks", "entities", "players", "trackingAndPackets", "respawn")
        const val NO_SECTION = Long.MIN_VALUE
        const val RANDOM_TICK_SPEED = 3

        fun sectionKey(x: Int, y: Int, z: Int): Long =
            ((x.toLong() and 0x3FFFFF) shl 42) or (y.toLong() and 0xFFFFF) or ((z.toLong() and 0x3FFFFF) shl 20)
    }
}

package bench

/**
 * A block type with overridable behaviour. Each behaviour is a virtual call through the block registry,
 * the same dispatch pattern as vanilla's `BlockBehaviour`.
 */
open class Block(val name: String) {
    var id = 0
        internal set

    open val isSolid: Boolean get() = true
    open val isRandomlyTicking: Boolean get() = false

    open fun randomTick(level: Level, pos: BlockPos, random: JavaRandom) {}
    open fun tick(level: Level, pos: BlockPos, random: JavaRandom) {}
    open fun onPlace(level: Level, pos: BlockPos) {}
    open fun neighborChanged(level: Level, pos: BlockPos, from: BlockPos) {}
}

class AirBlock : Block("air") {
    override val isSolid get() = false
}

class GrassBlock : Block("grass_block") {
    override val isRandomlyTicking get() = true

    override fun randomTick(level: Level, pos: BlockPos, random: JavaRandom) {
        if (level.getBlock(pos.above()).isSolid) {
            level.setBlock(pos, Blocks.DIRT)
            return
        }
        repeat(4) {
            val target = pos.offset(random.nextInt(3) - 1, random.nextInt(5) - 3, random.nextInt(3) - 1)
            if (level.getBlock(target) === Blocks.DIRT && !level.getBlock(target.above()).isSolid) {
                level.setBlock(target, Blocks.GRASS)
            }
        }
    }
}

/** Sand and gravel: schedule a tick when disturbed, then fall one block per tick. */
class FallingBlock(name: String) : Block(name) {
    override fun onPlace(level: Level, pos: BlockPos) = level.scheduleTick(pos, this, 2)

    override fun neighborChanged(level: Level, pos: BlockPos, from: BlockPos) = level.scheduleTick(pos, this, 2)

    override fun tick(level: Level, pos: BlockPos, random: JavaRandom) {
        val below = pos.below()
        if (below.y > 0 && !level.getBlock(below).isSolid) {
            level.setBlock(pos, Blocks.AIR)
            level.setBlock(below, this)
        }
    }
}

/** Flows down into air, and sideways over solid ground below sea level. */
class WaterBlock : Block("water") {
    override val isSolid get() = false

    override fun onPlace(level: Level, pos: BlockPos) = level.scheduleTick(pos, this, 5)

    override fun neighborChanged(level: Level, pos: BlockPos, from: BlockPos) = level.scheduleTick(pos, this, 5)

    override fun tick(level: Level, pos: BlockPos, random: JavaRandom) {
        val below = pos.below()
        if (below.y > 0 && level.getBlock(below) === Blocks.AIR) {
            level.setBlock(below, this)
            return
        }
        if (pos.y > Chunk.SEA_LEVEL) return
        for (direction in Direction.HORIZONTAL) {
            val side = pos.relative(direction)
            if (level.getBlock(side) === Blocks.AIR && level.getBlock(side.below()).isSolid) {
                level.setBlock(side, this)
            }
        }
    }
}

/** Decays when no log is within two blocks, dropping an item like vanilla leaves. */
class LeavesBlock : Block("oak_leaves") {
    override val isRandomlyTicking get() = true

    override fun randomTick(level: Level, pos: BlockPos, random: JavaRandom) {
        if (random.nextInt(8) != 0) return
        for (dx in -2..2) for (dy in -2..2) for (dz in -2..2) {
            if (level.getBlock(pos.offset(dx, dy, dz)) === Blocks.LOG) return
        }
        level.setBlock(pos, Blocks.AIR)
        level.addEntity(ItemEntity(level, id, 1).apply { setPos(pos.x + 0.5, pos.y + 0.5, pos.z + 0.5) })
    }
}

class CropBlock(val stage: Int) : Block("wheat_$stage") {
    override val isSolid get() = false
    override val isRandomlyTicking get() = stage < MAX_STAGE

    override fun randomTick(level: Level, pos: BlockPos, random: JavaRandom) {
        if (random.nextInt(3) == 0) level.setBlock(pos, Blocks.WHEAT[stage + 1])
    }

    companion object {
        const val MAX_STAGE = 7
    }
}

object Blocks {
    private val registry = ArrayList<Block>()

    private fun <T : Block> register(block: T): T {
        block.id = registry.size
        registry.add(block)
        return block
    }

    val AIR = register(AirBlock())
    val STONE = register(Block("stone"))
    val DIRT = register(Block("dirt"))
    val GRASS = register(GrassBlock())
    val SAND = register(FallingBlock("sand"))
    val GRAVEL = register(FallingBlock("gravel"))
    val WATER = register(WaterBlock())
    val LOG = register(Block("oak_log"))
    val LEAVES = register(LeavesBlock())
    val WHEAT: List<CropBlock> = (0..CropBlock.MAX_STAGE).map { register(CropBlock(it)) }

    private val byId: Array<Block> = registry.toTypedArray()

    fun byId(id: Int): Block = byId[id]
}

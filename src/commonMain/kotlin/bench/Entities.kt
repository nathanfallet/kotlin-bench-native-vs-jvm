package bench

import kotlin.math.sqrt

enum class EntityType(val width: Double, val height: Double) {
    ZOMBIE(0.6, 1.95), COW(0.9, 1.4), VILLAGER(0.6, 1.95), SKELETON(0.6, 1.99),
    ITEM(0.25, 0.25), ARROW(0.5, 0.5), PLAYER(0.6, 1.8);

    fun create(level: Level): Entity = when (this) {
        ZOMBIE -> Zombie(level)
        COW -> Cow(level)
        VILLAGER -> Villager(level)
        SKELETON -> Skeleton(level)
        ITEM -> ItemEntity(level, Blocks.DIRT.id, 1)
        ARROW -> Arrow(level, null)
        PLAYER -> error("players are created by the benchmark")
    }
}

abstract class Entity(val level: Level) {
    val id = level.nextEntityId++
    abstract val type: EntityType
    var x = 0.0
    var y = 0.0
    var z = 0.0
    var motion = Vec3.ZERO
    var bb = AABB(0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
    var onGround = false
    var horizontalCollision = false
    var removed = false
    var age = 0
    var yRot = 0f
    var xRot = 0f
    var sectionKey = Level.NO_SECTION

    // Position last sent to clients, and the delta computed once per tick for every tracking player.
    private var sentX = 0.0
    private var sentY = 0.0
    private var sentZ = 0.0
    var trackingDelta: MoveEntityPacket? = null
        private set

    fun setPos(x: Double, y: Double, z: Double) {
        this.x = x
        this.y = y
        this.z = z
        val halfWidth = type.width / 2
        bb = AABB(x - halfWidth, y, z - halfWidth, x + halfWidth, y + type.height, z + halfWidth)
        level.updateSection(this)
    }

    fun position() = Vec3(x, y, z)

    fun distanceToSqr(other: Entity): Double {
        val dx = x - other.x
        val dy = y - other.y
        val dz = z - other.z
        return dx * dx + dy * dy + dz * dz
    }

    open fun tick() {
        age++
    }

    fun remove() {
        if (removed) return
        removed = true
        level.onRemoved(this)
    }

    /** Moves with block collisions, resolving Y then X then Z like vanilla's `Entity.collide`. */
    fun move(dx: Double, dy: Double, dz: Double) {
        val shapes = level.getBlockCollisions(bb.expandTowards(dx, dy, dz))
        var box = bb
        var clippedY = dy
        for (shape in shapes) clippedY = shape.clipYCollide(box, clippedY)
        box = box.move(0.0, clippedY, 0.0)
        var clippedX = dx
        for (shape in shapes) clippedX = shape.clipXCollide(box, clippedX)
        box = box.move(clippedX, 0.0, 0.0)
        var clippedZ = dz
        for (shape in shapes) clippedZ = shape.clipZCollide(box, clippedZ)
        setPos(x + clippedX, y + clippedY, z + clippedZ)
        horizontalCollision = clippedX != dx || clippedZ != dz
        onGround = clippedY != dy && dy < 0
        if (clippedY != dy) motion = Vec3(motion.x, 0.0, motion.z)
    }

    fun prepareTrackingDelta() {
        val dx = ((x - sentX) * 4096.0).toLong()
        val dy = ((y - sentY) * 4096.0).toLong()
        val dz = ((z - sentZ) * 4096.0).toLong()
        trackingDelta = if (dx == 0L && dy == 0L && dz == 0L) {
            null
        } else {
            MoveEntityPacket(id, dx.toInt().toShort(), dy.toInt().toShort(), dz.toInt().toShort(), angle(yRot), angle(xRot), onGround)
        }
    }

    fun commitTrackingDelta() {
        if (trackingDelta == null) return
        sentX = x
        sentY = y
        sentZ = z
    }

    private fun angle(degrees: Float): Byte = (degrees * 256f / 360f).toInt().toByte()
}

abstract class LivingEntity(level: Level) : Entity(level) {
    abstract val maxHealth: Float
    var health = -1f
    var lastHurtTime = -1000L
    var lastAttacker: Entity? = null
    open val moveSpeed = 0.25
    var navigationTarget: Vec3? = null

    override fun tick() {
        super.tick()
        if (health < 0f) health = maxHealth
    }

    fun hurt(amount: Float, attacker: Entity?) {
        if (removed) return
        health -= amount
        lastHurtTime = level.gameTime
        lastAttacker = attacker
        if (attacker != null) {
            val push = Vec3(x - attacker.x, 0.0, z - attacker.z).normalize().scale(0.4)
            motion = Vec3(motion.x + push.x, 0.36, motion.z + push.z)
        }
        if (health <= 0f) die()
    }

    open fun die() {
        repeat(1 + level.random.nextInt(2)) {
            level.addEntity(ItemEntity(level, Blocks.DIRT.id + it, 1).apply { setPos(this@LivingEntity.x, this@LivingEntity.y + 0.5, this@LivingEntity.z) })
        }
        remove()
    }

    /** Walks towards [navigationTarget], jumps over obstacles, applies gravity and friction. */
    fun travel() {
        var mx = motion.x
        var mz = motion.z
        val target = navigationTarget
        if (target != null) {
            val dx = target.x - x
            val dz = target.z - z
            val distance = sqrt(dx * dx + dz * dz)
            if (distance > 0.6) {
                mx += dx / distance * moveSpeed * 0.2
                mz += dz / distance * moveSpeed * 0.2
                yRot = (fastAtan2(dz, dx) * 57.2957763671875).toFloat() - 90f
            } else {
                navigationTarget = null
            }
        }
        var my = motion.y - 0.08
        if (horizontalCollision && onGround) my = 0.42
        motion = Vec3(mx, my, mz)
        move(motion.x, motion.y, motion.z)
        val friction = if (onGround) 0.546 else 0.91
        motion = Vec3(motion.x * friction, motion.y * 0.98, motion.z * friction)
    }
}

// ---------------------------------------------------------------- goals

abstract class Goal(val priority: Int, val flags: Int) {
    var running = false
    abstract fun canUse(): Boolean
    open fun canContinueToUse(): Boolean = canUse()
    open fun start() {}
    open fun stop() {}
    open fun tick() {}

    companion object {
        const val MOVE = 1
        const val LOOK = 2
        const val TARGET = 4
    }
}

/** Mirrors vanilla's `GoalSelector`: lambdas and collection pipelines every other tick, a lot of short-lived garbage. */
class GoalSelector(private val goals: List<Goal>) {
    fun update() {
        goals.filter { it.running && !it.canContinueToUse() }.forEach {
            it.stop()
            it.running = false
        }
        goals.asSequence()
            .filter { !it.running }
            .filter { candidate -> goals.none { it.running && it.flags and candidate.flags != 0 && it.priority <= candidate.priority } }
            .filter { it.canUse() }
            .toList()
            .forEach { candidate ->
                goals.filter { it.running && it.flags and candidate.flags != 0 }.forEach {
                    it.stop()
                    it.running = false
                }
                candidate.start()
                candidate.running = true
            }
        tickRunning()
    }

    fun tickRunning() {
        for (goal in goals) if (goal.running) goal.tick()
    }
}

abstract class Mob(level: Level) : LivingEntity(level) {
    var target: LivingEntity? = null
    private val selector by lazy { GoalSelector(createGoals().sortedBy { it.priority }) }

    abstract fun createGoals(): List<Goal>

    override fun tick() {
        super.tick()
        if (target?.removed == true) target = null
        if (age % 2 == 0) selector.update() else selector.tickRunning()
        travel()
    }
}

class RandomStrollGoal(private val mob: Mob, priority: Int) : Goal(priority, MOVE) {
    private var ticks = 0
    override fun canUse() = mob.navigationTarget == null && mob.level.random.nextInt(120) == 0
    override fun canContinueToUse() = mob.navigationTarget != null && ticks < 200
    override fun start() {
        ticks = 0
        val random = mob.level.random
        val x = (mob.x + random.nextInt(21) - 10).coerceIn(2.0, mob.level.widthInBlocks - 2.0)
        val z = (mob.z + random.nextInt(21) - 10).coerceIn(2.0, mob.level.widthInBlocks - 2.0)
        mob.navigationTarget = Vec3(x, mob.level.surfaceY(floorInt(x), floorInt(z)) + 1.0, z)
    }
    override fun tick() {
        ticks++
    }
    override fun stop() {
        mob.navigationTarget = null
    }
}

class LookAtNearestGoal(private val mob: Mob, priority: Int) : Goal(priority, LOOK) {
    private var lookAt: Entity? = null
    private var ticks = 0
    override fun canUse(): Boolean {
        if (mob.level.random.nextFloat() >= 0.02f) return false
        lookAt = mob.level.getEntities(mob, mob.bb.inflate(8.0, 3.0, 8.0)) { it is LivingEntity }
            .minByOrNull { mob.distanceToSqr(it) }
        return lookAt != null
    }
    override fun canContinueToUse() = lookAt?.removed == false && ticks < 60
    override fun start() {
        ticks = 0
    }
    override fun tick() {
        ticks++
        val other = lookAt ?: return
        mob.yRot = (fastAtan2(other.z - mob.z, other.x - mob.x) * 57.2957763671875).toFloat() - 90f
        mob.xRot = (fastAtan2(other.y - mob.y, sqrt(mob.distanceToSqr(other))) * -57.2957763671875).toFloat()
    }
}

class NearestTargetGoal(private val mob: Mob, priority: Int, private val targets: Set<EntityType>) : Goal(priority, TARGET) {
    override fun canUse(): Boolean {
        if (mob.target != null || mob.level.random.nextInt(10) != 0) return false
        mob.target = mob.level.getEntities(mob, mob.bb.inflate(16.0, 4.0, 16.0)) { it.type in targets }
            .minByOrNull { mob.distanceToSqr(it) } as LivingEntity?
        return mob.target != null
    }
    override fun canContinueToUse() = mob.target?.removed == false && mob.distanceToSqr(mob.target!!) < 24.0 * 24.0
    override fun stop() {
        mob.target = null
    }
}

class MeleeAttackGoal(private val mob: Mob, priority: Int) : Goal(priority, MOVE or LOOK) {
    private var cooldown = 0
    override fun canUse() = mob.target != null
    override fun tick() {
        val target = mob.target ?: return
        mob.navigationTarget = target.position()
        if (--cooldown <= 0 && mob.distanceToSqr(target) < 2.5) {
            target.hurt(3f, mob)
            cooldown = 20
        }
    }
    override fun stop() {
        mob.navigationTarget = null
    }
}

class RangedAttackGoal(private val mob: Mob, priority: Int) : Goal(priority, MOVE or LOOK) {
    private var cooldown = 0
    override fun canUse() = mob.target != null
    override fun tick() {
        val target = mob.target ?: return
        val distanceSqr = mob.distanceToSqr(target)
        mob.navigationTarget = if (distanceSqr > 100.0) target.position() else null
        if (--cooldown <= 0 && distanceSqr < 225.0) {
            val arrow = Arrow(mob.level, mob)
            arrow.setPos(mob.x, mob.y + 1.5, mob.z)
            val aim = Vec3(target.x - mob.x, target.y + 1.0 - (mob.y + 1.5), target.z - mob.z)
            arrow.motion = aim.normalize().scale(1.6) + Vec3(0.0, sqrt(distanceSqr) * 0.012, 0.0)
            mob.level.addEntity(arrow)
            cooldown = 40
        }
    }
    override fun stop() {
        mob.navigationTarget = null
    }
}

class PanicGoal(private val mob: Mob, priority: Int) : Goal(priority, MOVE) {
    override fun canUse() = mob.level.gameTime - mob.lastHurtTime < 60
    override fun start() {
        val random = mob.level.random
        mob.navigationTarget = Vec3(
            (mob.x + random.nextInt(11) - 5).coerceIn(2.0, mob.level.widthInBlocks - 2.0),
            mob.y,
            (mob.z + random.nextInt(11) - 5).coerceIn(2.0, mob.level.widthInBlocks - 2.0),
        )
    }
}

/** Looks for mature wheat in a 13x3x13 area: hundreds of block lookups, each with an allocated position. */
class HarvestCropsGoal(private val mob: Mob, priority: Int) : Goal(priority, MOVE) {
    private var crop: BlockPos? = null
    override fun canUse(): Boolean {
        if (mob.level.random.nextInt(40) != 0) return false
        val origin = BlockPos(floorInt(mob.x), floorInt(mob.y), floorInt(mob.z))
        val mature = Blocks.WHEAT[CropBlock.MAX_STAGE]
        crop = null
        for (dx in -6..6) for (dy in -1..1) for (dz in -6..6) {
            val pos = origin.offset(dx, dy, dz)
            if (mob.level.getBlock(pos) === mature) {
                crop = pos
                return true
            }
        }
        return false
    }
    override fun canContinueToUse() = crop != null && mob.level.getBlock(crop!!) === Blocks.WHEAT[CropBlock.MAX_STAGE]
    override fun tick() {
        val pos = crop ?: return
        mob.navigationTarget = Vec3(pos.x + 0.5, pos.y.toDouble(), pos.z + 0.5)
        val dx = pos.x + 0.5 - mob.x
        val dz = pos.z + 0.5 - mob.z
        if (dx * dx + dz * dz < 2.0) {
            mob.level.setBlock(pos, Blocks.WHEAT[0])
            mob.level.addEntity(ItemEntity(mob.level, Blocks.WHEAT[CropBlock.MAX_STAGE].id, 2).apply { setPos(pos.x + 0.5, pos.y + 0.5, pos.z + 0.5) })
            crop = null
        }
    }
    override fun stop() {
        mob.navigationTarget = null
    }
}

// ---------------------------------------------------------------- concrete entities

class Zombie(level: Level) : Mob(level) {
    override val type get() = EntityType.ZOMBIE
    override val maxHealth get() = 20f
    override val moveSpeed get() = 0.23
    override fun createGoals() = listOf(
        NearestTargetGoal(this, 2, setOf(EntityType.VILLAGER, EntityType.COW, EntityType.PLAYER)),
        MeleeAttackGoal(this, 3),
        RandomStrollGoal(this, 7),
        LookAtNearestGoal(this, 8),
    )
}

class Skeleton(level: Level) : Mob(level) {
    override val type get() = EntityType.SKELETON
    override val maxHealth get() = 20f
    override fun createGoals() = listOf(
        NearestTargetGoal(this, 2, setOf(EntityType.ZOMBIE, EntityType.PLAYER)),
        RangedAttackGoal(this, 3),
        RandomStrollGoal(this, 7),
        LookAtNearestGoal(this, 8),
    )
}

class Cow(level: Level) : Mob(level) {
    override val type get() = EntityType.COW
    override val maxHealth get() = 10f
    override val moveSpeed get() = 0.2
    override fun createGoals() = listOf(PanicGoal(this, 1), RandomStrollGoal(this, 6), LookAtNearestGoal(this, 7))
}

class Villager(level: Level) : Mob(level) {
    override val type get() = EntityType.VILLAGER
    override val maxHealth get() = 20f
    override val moveSpeed get() = 0.5
    override fun createGoals() = listOf(
        PanicGoal(this, 1),
        HarvestCropsGoal(this, 3),
        RandomStrollGoal(this, 6),
        LookAtNearestGoal(this, 8),
    )
}

class ItemEntity(level: Level, val itemId: Int, var count: Int) : Entity(level) {
    override val type get() = EntityType.ITEM

    override fun tick() {
        super.tick()
        motion = Vec3(motion.x, motion.y - 0.04, motion.z)
        move(motion.x, motion.y, motion.z)
        val friction = if (onGround) 0.588 else 0.98
        motion = Vec3(motion.x * friction, motion.y * 0.98, motion.z * friction)
        if (age % 20 == 0) {
            level.getEntities(this, bb.inflate(0.5, 0.0, 0.5)) { it is ItemEntity && it.itemId == itemId }.forEach {
                count += (it as ItemEntity).count
                it.remove()
            }
        }
        if (age >= 1200) remove()
    }
}

class Arrow(level: Level, private val owner: Entity?) : Entity(level) {
    override val type get() = EntityType.ARROW
    private var inGroundTicks = -1

    override fun tick() {
        super.tick()
        if (inGroundTicks >= 0) {
            if (++inGroundTicks > 200) remove()
            return
        }
        val start = position()
        val end = start + motion
        for (step in 1..4) {
            val point = start + motion.scale(step / 4.0)
            if (level.getBlock(BlockPos(floorInt(point.x), floorInt(point.y), floorInt(point.z))).isSolid) {
                setPos(point.x, point.y, point.z)
                inGroundTicks = 0
                return
            }
        }
        val hit = level.getEntities(this, bb.expandTowards(motion.x, motion.y, motion.z).inflate(0.3)) {
            it is LivingEntity && it !== owner
        }.firstOrNull() as LivingEntity?
        if (hit != null) {
            hit.hurt(4f, owner)
            remove()
            return
        }
        setPos(end.x, end.y, end.z)
        motion = Vec3(motion.x * 0.99, motion.y * 0.99 - 0.05, motion.z * 0.99)
        if (y < 0 || age > 400) remove()
    }
}

/** A scripted player: walks around, breaks and places blocks, picks items up, and owns a connection. */
class ServerPlayer(level: Level) : LivingEntity(level) {
    override val type get() = EntityType.PLAYER
    override val maxHealth get() = 20f
    override val moveSpeed get() = 0.5
    val connection = Connection(this)
    private var inventory = 0L

    override fun tick() {
        super.tick()
        val random = level.random
        if (health < maxHealth) health = maxHealth
        if (age % 60 == 1 || navigationTarget == null) {
            val x = (this.x + random.nextInt(33) - 16).coerceIn(4.0, level.widthInBlocks - 4.0)
            val z = (this.z + random.nextInt(33) - 16).coerceIn(4.0, level.widthInBlocks - 4.0)
            navigationTarget = Vec3(x, y, z)
        }
        travel()
        if (age % 20 == 0) {
            val pos = BlockPos(floorInt(x) + random.nextInt(7) - 3, floorInt(y) + random.nextInt(4) - 2, floorInt(z) + random.nextInt(7) - 3)
            val block = level.getBlock(pos)
            if (block !== Blocks.AIR && block !== Blocks.WATER) {
                level.setBlock(pos, Blocks.AIR)
                level.addEntity(ItemEntity(level, block.id, 1).apply { setPos(pos.x + 0.5, pos.y + 0.5, pos.z + 0.5) })
            }
        }
        if (age % 30 == 0) {
            val pos = BlockPos(floorInt(x) + random.nextInt(5) - 2, floorInt(y) + random.nextInt(3), floorInt(z) + random.nextInt(5) - 2)
            if (level.getBlock(pos) === Blocks.AIR) level.setBlock(pos, if (random.nextBoolean()) Blocks.SAND else Blocks.GRAVEL)
        }
        level.getEntities(this, bb.inflate(1.0, 0.5, 1.0)) { it is ItemEntity && it.age > 10 }.forEach {
            inventory += (it as ItemEntity).count
            it.remove()
        }
    }
}

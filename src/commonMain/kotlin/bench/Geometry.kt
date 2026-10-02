package bench

import kotlin.math.max
import kotlin.math.min
import kotlin.math.sqrt

/** Immutable block position. Like vanilla, the code allocates a new one for every neighbour lookup. */
data class BlockPos(val x: Int, val y: Int, val z: Int) {
    fun above() = BlockPos(x, y + 1, z)
    fun below() = BlockPos(x, y - 1, z)
    fun offset(dx: Int, dy: Int, dz: Int) = BlockPos(x + dx, y + dy, z + dz)
    fun relative(direction: Direction) = BlockPos(x + direction.dx, y + direction.dy, z + direction.dz)
    fun asLong(): Long = ((x.toLong() and 0x3FFFFFF) shl 38) or ((z.toLong() and 0x3FFFFFF) shl 12) or (y.toLong() and 0xFFF)
    fun distSqr(other: BlockPos): Int {
        val dx = x - other.x
        val dy = y - other.y
        val dz = z - other.z
        return dx * dx + dy * dy + dz * dz
    }
}

enum class Direction(val dx: Int, val dy: Int, val dz: Int) {
    DOWN(0, -1, 0), UP(0, 1, 0), NORTH(0, 0, -1), SOUTH(0, 0, 1), WEST(-1, 0, 0), EAST(1, 0, 0);

    companion object {
        val HORIZONTAL = listOf(NORTH, SOUTH, WEST, EAST)
    }
}

data class Vec3(val x: Double, val y: Double, val z: Double) {
    operator fun plus(o: Vec3) = Vec3(x + o.x, y + o.y, z + o.z)
    operator fun minus(o: Vec3) = Vec3(x - o.x, y - o.y, z - o.z)
    fun scale(f: Double) = Vec3(x * f, y * f, z * f)
    fun length() = sqrt(x * x + y * y + z * z)
    fun normalize(): Vec3 {
        val length = length()
        return if (length < 1.0E-4) ZERO else Vec3(x / length, y / length, z / length)
    }

    companion object {
        val ZERO = Vec3(0.0, 0.0, 0.0)
    }
}

/** Axis-aligned bounding box, with the three `clip*Collide` sweeps vanilla uses to resolve movement. */
class AABB(
    val minX: Double, val minY: Double, val minZ: Double,
    val maxX: Double, val maxY: Double, val maxZ: Double,
) {
    fun move(dx: Double, dy: Double, dz: Double) = AABB(minX + dx, minY + dy, minZ + dz, maxX + dx, maxY + dy, maxZ + dz)

    fun inflate(x: Double, y: Double, z: Double) = AABB(minX - x, minY - y, minZ - z, maxX + x, maxY + y, maxZ + z)

    fun inflate(amount: Double) = inflate(amount, amount, amount)

    fun expandTowards(dx: Double, dy: Double, dz: Double) = AABB(
        if (dx < 0) minX + dx else minX, if (dy < 0) minY + dy else minY, if (dz < 0) minZ + dz else minZ,
        if (dx > 0) maxX + dx else maxX, if (dy > 0) maxY + dy else maxY, if (dz > 0) maxZ + dz else maxZ,
    )

    fun intersects(o: AABB) =
        minX < o.maxX && maxX > o.minX && minY < o.maxY && maxY > o.minY && minZ < o.maxZ && maxZ > o.minZ

    fun clipXCollide(other: AABB, dx: Double): Double {
        if (other.maxY <= minY || other.minY >= maxY || other.maxZ <= minZ || other.minZ >= maxZ) return dx
        return when {
            dx > 0 && other.maxX <= minX -> min(dx, minX - other.maxX)
            dx < 0 && other.minX >= maxX -> max(dx, maxX - other.minX)
            else -> dx
        }
    }

    fun clipYCollide(other: AABB, dy: Double): Double {
        if (other.maxX <= minX || other.minX >= maxX || other.maxZ <= minZ || other.minZ >= maxZ) return dy
        return when {
            dy > 0 && other.maxY <= minY -> min(dy, minY - other.maxY)
            dy < 0 && other.minY >= maxY -> max(dy, maxY - other.minY)
            else -> dy
        }
    }

    fun clipZCollide(other: AABB, dz: Double): Double {
        if (other.maxX <= minX || other.minX >= maxX || other.maxY <= minY || other.minY >= maxY) return dz
        return when {
            dz > 0 && other.maxZ <= minZ -> min(dz, minZ - other.maxZ)
            dz < 0 && other.minZ >= maxZ -> max(dz, maxZ - other.minZ)
            else -> dz
        }
    }

    companion object {
        fun ofBlock(x: Int, y: Int, z: Int) =
            AABB(x.toDouble(), y.toDouble(), z.toDouble(), x + 1.0, y + 1.0, z + 1.0)
    }
}

package bench

/** Growable big-endian byte buffer with Minecraft's VarInt encoding. */
class PacketBuffer(initialCapacity: Int = 256) {
    private var data = ByteArray(initialCapacity)
    var size = 0
        private set

    private fun ensure(extra: Int) {
        if (size + extra > data.size) data = data.copyOf(maxOf(data.size * 2, size + extra))
    }

    fun writeByte(value: Int) {
        ensure(1)
        data[size++] = value.toByte()
    }

    fun writeVarInt(value: Int) {
        var v = value
        while (v and 0x7F.inv() != 0) {
            writeByte((v and 0x7F) or 0x80)
            v = v ushr 7
        }
        writeByte(v)
    }

    fun writeShort(value: Int) {
        writeByte(value shr 8)
        writeByte(value)
    }

    fun writeLong(value: Long) {
        for (shift in 56 downTo 0 step 8) writeByte((value shr shift).toInt())
    }

    fun writeDouble(value: Double) = writeLong(value.toRawBits())

    fun writeBytes(other: PacketBuffer) {
        ensure(other.size)
        other.data.copyInto(data, size, 0, other.size)
        size += other.size
    }

    fun hash(): Long {
        var hash = -0x340d631b7bdddcdbL
        for (i in 0 until size) hash = mixHash(hash, data[i].toLong())
        return hash
    }

    fun clear() {
        size = 0
    }
}

sealed class Packet(val id: Int) {
    abstract fun write(buffer: PacketBuffer)
}

class AddEntityPacket(val entityId: Int, val entityType: Int, val x: Double, val y: Double, val z: Double) : Packet(0x01) {
    override fun write(buffer: PacketBuffer) {
        buffer.writeVarInt(entityId)
        buffer.writeVarInt(entityType)
        buffer.writeDouble(x)
        buffer.writeDouble(y)
        buffer.writeDouble(z)
    }
}

class MoveEntityPacket(
    val entityId: Int, val dx: Short, val dy: Short, val dz: Short,
    val yRot: Byte, val xRot: Byte, val onGround: Boolean,
) : Packet(0x2F) {
    override fun write(buffer: PacketBuffer) {
        buffer.writeVarInt(entityId)
        buffer.writeShort(dx.toInt())
        buffer.writeShort(dy.toInt())
        buffer.writeShort(dz.toInt())
        buffer.writeByte(yRot.toInt())
        buffer.writeByte(xRot.toInt())
        buffer.writeByte(if (onGround) 1 else 0)
    }
}

class RemoveEntitiesPacket(val ids: IntArray) : Packet(0x46) {
    override fun write(buffer: PacketBuffer) {
        buffer.writeVarInt(ids.size)
        for (id in ids) buffer.writeVarInt(id)
    }
}

class BlockUpdatePacket(val pos: Long, val state: Int) : Packet(0x09) {
    override fun write(buffer: PacketBuffer) {
        buffer.writeLong(pos)
        buffer.writeVarInt(state)
    }
}

/**
 * Per-player entity tracking and packet encoding: which entities entered or left view distance,
 * which moved, and which blocks changed nearby. Returns a hash of the bytes that would go on the wire.
 */
class Connection(private val player: ServerPlayer) {
    private val tracked = HashSet<Int>()
    private val pending = ArrayList<Packet>()
    private val frame = PacketBuffer(4096)
    private val scratch = PacketBuffer(256)

    fun sendChanges(level: Level): Long {
        val visible = level.getEntities(player, player.bb.inflate(VIEW_DISTANCE, VIEW_DISTANCE, VIEW_DISTANCE)) { true }
        val visibleIds = HashSet<Int>(visible.size * 2)
        for (entity in visible) {
            visibleIds.add(entity.id)
            if (tracked.add(entity.id)) {
                pending.add(AddEntityPacket(entity.id, entity.type.ordinal, entity.x, entity.y, entity.z))
            } else {
                entity.trackingDelta?.let { pending.add(it) }
            }
        }
        val gone = tracked.filter { it !in visibleIds }.sorted()
        if (gone.isNotEmpty()) {
            pending.add(RemoveEntitiesPacket(gone.toIntArray()))
            tracked.removeAll(gone.toSet())
        }
        for (pos in level.changedBlocks) {
            val dx = pos.x - player.x
            val dz = pos.z - player.z
            if (dx * dx + dz * dz < BLOCK_UPDATE_DISTANCE_SQR) {
                pending.add(BlockUpdatePacket(pos.asLong(), level.getBlock(pos).id))
            }
        }
        return flush()
    }

    /** Frames every pending packet as `length | id | payload`, the uncompressed vanilla wire format. */
    private fun flush(): Long {
        for (packet in pending) {
            scratch.clear()
            scratch.writeVarInt(packet.id)
            packet.write(scratch)
            frame.writeVarInt(scratch.size)
            frame.writeBytes(scratch)
        }
        pending.clear()
        val hash = frame.hash()
        frame.clear()
        return hash
    }

    private companion object {
        const val VIEW_DISTANCE = 48.0
        const val BLOCK_UPDATE_DISTANCE_SQR = 64.0 * 64.0
    }
}

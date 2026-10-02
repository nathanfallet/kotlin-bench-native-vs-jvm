package bench

/**
 * Open-addressing `Long -> V` map without boxed keys, modelled on fastutil's `Long2ObjectOpenHashMap`,
 * which vanilla uses for chunks and entity sections. Iteration order depends only on the keys,
 * so it is identical on every target.
 */
class LongObjectMap<V : Any>(expected: Int = 16) {
    private var capacity = tableSize(expected)
    private var mask = capacity - 1
    private var keys = LongArray(capacity)
    private var values = arrayOfNulls<Any>(capacity)
    private var filled = BooleanArray(capacity)
    var size = 0
        private set

    @Suppress("UNCHECKED_CAST")
    operator fun get(key: Long): V? {
        var pos = mix(key) and mask
        while (filled[pos]) {
            if (keys[pos] == key) return values[pos] as V
            pos = (pos + 1) and mask
        }
        return null
    }

    @Suppress("UNCHECKED_CAST")
    fun put(key: Long, value: V): V? {
        var pos = mix(key) and mask
        while (filled[pos]) {
            if (keys[pos] == key) {
                val old = values[pos] as V
                values[pos] = value
                return old
            }
            pos = (pos + 1) and mask
        }
        filled[pos] = true
        keys[pos] = key
        values[pos] = value
        if (++size > capacity * 3 / 4) rehash(capacity * 2)
        return null
    }

    inline fun getOrPut(key: Long, create: () -> V): V = get(key) ?: create().also { put(key, it) }

    @Suppress("UNCHECKED_CAST")
    fun remove(key: Long): V? {
        var pos = mix(key) and mask
        while (filled[pos]) {
            if (keys[pos] == key) {
                val old = values[pos] as V
                size--
                shiftKeys(pos)
                return old
            }
            pos = (pos + 1) and mask
        }
        return null
    }

    @Suppress("UNCHECKED_CAST")
    inline fun forEachValue(action: (V) -> Unit) {
        for (i in 0 until slotCount()) {
            val value = valueAt(i)
            if (value != null) action(value as V)
        }
    }

    fun slotCount(): Int = capacity

    fun valueAt(slot: Int): Any? = if (filled[slot]) values[slot] else null

    private fun shiftKeys(start: Int) {
        var last = start
        var pos = start
        while (true) {
            pos = (pos + 1) and mask
            while (true) {
                if (!filled[pos]) {
                    filled[last] = false
                    values[last] = null
                    return
                }
                val slot = mix(keys[pos]) and mask
                val stays = if (last <= pos) last >= slot || slot > pos else last >= slot && slot > pos
                if (stays) break
                pos = (pos + 1) and mask
            }
            keys[last] = keys[pos]
            values[last] = values[pos]
            filled[last] = true
            last = pos
        }
    }

    private fun rehash(newCapacity: Int) {
        val oldKeys = keys
        val oldValues = values
        val oldFilled = filled
        capacity = newCapacity
        mask = newCapacity - 1
        keys = LongArray(newCapacity)
        values = arrayOfNulls(newCapacity)
        filled = BooleanArray(newCapacity)
        for (i in oldKeys.indices) {
            if (!oldFilled[i]) continue
            var pos = mix(oldKeys[i]) and mask
            while (filled[pos]) pos = (pos + 1) and mask
            filled[pos] = true
            keys[pos] = oldKeys[i]
            values[pos] = oldValues[i]
        }
    }

    private companion object {
        fun tableSize(expected: Int): Int {
            var size = 16
            while (size * 3 / 4 < expected) size *= 2
            return size
        }

        fun mix(key: Long): Int {
            val h = key * -0x61c8864680b583ebL
            val folded = h xor (h ushr 32)
            return (folded xor (folded ushr 16)).toInt()
        }
    }
}

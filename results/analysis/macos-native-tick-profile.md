
## Thread: unnamed  (12055 samples, 100% busy)

| Bucket | Share of busy time |
|---|---:|
| game code (bench.*) | 57.1% |
| kotlin.collections (HashMap, ArrayList, iterators) | 21.9% |
| thread-local access (_tlv_get_addr) | 11.7% |
| allocation | 6.4% |
| boxing and equals bridges | 1.3% |
| lazy / global init checks | 0.8% |
| other runtime / libc | 0.5% |
| GC: sweep | 0.4% |

Top self-time frames:

-  8.4%  kfun:bench.Connection#sendChanges(bench.Level){}kotlin.Long  (in bench.kexe) + 8352,8288,...  [0x1022b8be4,0x1
-  7.3%  _tlv_get_addr
-  5.2%  kfun:bench.Entity#move(kotlin.Double;kotlin.Double;kotlin.Double){}  (in bench.kexe) + 2688,1944,...  [0x10229
-  3.9%  kfun:bench.ItemEntity#tick(){}  (in bench.kexe) + 3292,3480,...  [0x1022a6588,0x1022a6644,...]  Entities.kt:41
-  3.6%  kfun:bench.Entity#move(kotlin.Double;kotlin.Double;kotlin.Double){}  (in bench.kexe) + 2688,2276,...  [0x10229
-  3.0%  kfun:bench.Level#tick(){}  (in bench.kexe) + 1916,1852,...  [0x1022ad1c8,0x1022ad188,...]  Level.kt:168
-  2.0%  kfun:bench.Mob#tick(){}-impl  (in bench.kexe) + 1100,888,...  [0x10229eca0,0x10229ebcc,...]  Entities.kt:222
-  1.6%  kotlin::alloc::CustomAllocator::Allocate(kotlin::alloc::AllocationSize)  (in bench.kexe) + 340,24,...  [0x1022
-  1.5%  _tlv_get_addr  (in libdyld.dylib) + 12,4,...  [0x18a0b4394,0x18a0b438c,...]
-  1.3%  kfun:kotlin.collections.HashMap.KeysItr#next(){}1:0  (in bench.kexe) + 308,372,...  [0x10220f740,0x10220f780,.
-  1.2%  kfun:bench.WaterBlock#<get-isSolid>(){}kotlin.Boolean  (in bench.kexe) + 0,4  [0x102299500,0x102299504]  Block
-  1.2%  kfun:bench.AirBlock#<get-isSolid>(){}kotlin.Boolean  (in bench.kexe) + 0,4  [0x1022983dc,0x1022983e0]  Blocks.
-  1.1%  kotlin::alloc::CustomAllocator::Allocate(kotlin::alloc::AllocationSize)  (in bench.kexe) + 2332,2320,...  [0x1
-  1.1%  kfun:bench.Level#getBlock(bench.BlockPos){}bench.Block  (in bench.kexe) + 392,388  [0x1022ab97c,0x1022ab978]  
-  1.1%  kfun:bench.Block#<get-isSolid>(){}kotlin.Boolean-impl  (in bench.kexe) + 0,4,...  [0x102298334,0x102298338,...

## Thread: Main GC thread  (12055 samples, 21% busy)

| Bucket | Share of busy time |
|---|---:|
| GC: mark | 54.5% |
| GC: sweep | 33.1% |
| allocation | 7.3% |
| other runtime / libc | 5.1% |

Top self-time frames:

- 33.1%  kotlin::alloc::FixedBlockPage::Sweep<kotlin::alloc::ObjectSweepTraits>(kotlin::alloc::ObjectSweepTraits::GCSwe
- 25.0%  kotlin::gc::Mark<kotlin::gc::mark::ConcurrentMark::MarkTraits>(kotlin::gc::GCHandle::GCMarkScope&, kotlin::gc:
-  9.6%  kotlin::gc::internal::processFieldInMark<kotlin::gc::mark::ConcurrentMark::MarkTraits>(void*, ObjHeader*, ObjH
-  8.1%  Kotlin_processObjectInMark  (in bench.kexe) + 56,0,...  [0x1022e6fb0,0x1022e6f78,...]
-  5.9%  _platform_memset  (in libsystem_platform.dylib) + 140,160,...  [0x18a4b311c,0x18a4b3130,...]
-  5.3%  Kotlin_processArrayInMark  (in bench.kexe) + 44,84,...  [0x1022e701c,0x1022e7044,...]
-  3.8%  kotlin::gc::internal::processFieldInMark<kotlin::gc::mark::ConcurrentMark::MarkTraits>(void*, ObjHeader*, ObjH
-  1.9%  Kotlin_processEmptyObjectInMark
-  1.8%  kotlin::gc::internal::processFieldInMark<kotlin::gc::mark::ConcurrentMark::MarkTraits>(void*, ObjHeader*, ObjH
-  1.4%  kotlin::gc::internal::MainGCThread<kotlin::gc::internal::CmsGCTraits>::PerformFullGC(long long)  (in bench.kex
-  1.1%  _platform_memset  (in libsystem_platform.dylib) + 140,148
-  0.7%  Kotlin_processObjectInMark  (in bench.kexe) + 112,116
-  0.7%  __bzero  (in libsystem_platform.dylib) + 68,24,...  [0x18a4b3074,0x18a4b3048,...]
-  0.5%  DYLD-STUB$$bzero

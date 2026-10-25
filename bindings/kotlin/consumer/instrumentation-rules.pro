# AndroidX instrumentation executes in the minified app process, sharing its Kotlin runtime.
# Keep this cross-APK entrypoint stable; the rest of the consumer still goes through R8.
-keep class kotlin.jvm.internal.Intrinsics { *; }

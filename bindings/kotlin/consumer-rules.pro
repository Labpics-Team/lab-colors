# JNA использует отражение для структур, callback и имён экспортированных функций.
-keep class com.sun.jna.** { *; }
-keep interface pics.lab.colors.UniffiLib { *; }
-keep class pics.lab.colors.** extends com.sun.jna.Structure { *; }
-keep class pics.lab.colors.** implements com.sun.jna.Callback { *; }
-keep interface pics.lab.colors.** extends com.sun.jna.Callback { *; }
-keepattributes RuntimeVisibleAnnotations,AnnotationDefault,InnerClasses,EnclosingMethod

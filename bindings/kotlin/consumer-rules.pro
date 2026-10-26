# JNA использует отражение для структур, callback и имён экспортированных функций.
# Необязательный desktop AWT-мост JNA не используется на Android.
# https://github.com/java-native-access/jna/blob/5.14.0/www/FrequentlyAskedQuestions.md#jna-on-android
-dontwarn java.awt.*
-keep class com.sun.jna.** { *; }
-keep interface pics.lab.colors.UniffiLib { *; }
-keep class pics.lab.colors.** extends com.sun.jna.Structure { *; }
-keep class pics.lab.colors.** implements com.sun.jna.Callback { *; }
-keep interface pics.lab.colors.** extends com.sun.jna.Callback { *; }
-keepattributes RuntimeVisibleAnnotations,AnnotationDefault,InnerClasses,EnclosingMethod

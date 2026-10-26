plugins {
    id("com.android.library") version "8.13.2"
    kotlin("android") version "2.2.21"
    `maven-publish`
}
group = "pics.lab"
version = providers.gradleProperty("publicationVersion").get()
android {
    namespace = "pics.lab.colors"
    compileSdk = 36
    defaultConfig { minSdk = 23; consumerProguardFiles("consumer-rules.pro") }
    sourceSets.getByName("main") {
        java.srcDir("generated")
        jniLibs.srcDir("jniLibs")
    }
    compileOptions {
        isCoreLibraryDesugaringEnabled = true
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
    // Производитель уже удалил отладочные символы и проверяет точные байты.
    packaging { jniLibs.keepDebugSymbols.add("**/liblabcolors.so") }
    publishing { singleVariant("release") { withSourcesJar() } }
}
kotlin { compilerOptions { jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_1_8 } }
dependencies {
    coreLibraryDesugaring("com.android.tools:desugar_jdk_libs:2.1.5")
    api("net.java.dev.jna:jna:5.19.1@aar")
    compileOnly("androidx.annotation:annotation:1.9.1")
}
afterEvaluate {
    publishing {
        publications {
            create<MavenPublication>("release") {
                from(components["release"])
                artifactId = "colors-android"
                pom {
                    name = "Lab Colors for Android"
                    description = "Adaptive Program runtime backed by the shared Lab Colors Core"
                    url = "https://github.com/Labpics-Team/lab-colors"
                }
            }
        }
        repositories { maven { url = uri(providers.gradleProperty("publicationRepository").get()) } }
    }
}

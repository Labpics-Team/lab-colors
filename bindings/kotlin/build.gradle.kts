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
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
    publishing { singleVariant("release") { withSourcesJar() } }
}
kotlin { compilerOptions { jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_1_8 } }
dependencies { api("net.java.dev.jna:jna:5.18.1@aar") }
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

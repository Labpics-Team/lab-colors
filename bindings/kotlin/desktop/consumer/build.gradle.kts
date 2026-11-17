plugins {
    kotlin("jvm") version "2.2.21"
    application
}
repositories {
    maven { url = uri(providers.gradleProperty("colorsRepository").get()) }
    mavenCentral()
}
kotlin { compilerOptions { jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17 } }
java { sourceCompatibility = JavaVersion.VERSION_17; targetCompatibility = JavaVersion.VERSION_17 }
dependencies { implementation("pics.lab:colors-desktop:${providers.gradleProperty("colorsVersion").get()}") }
application { mainClass.set("consumer.MainKt") }

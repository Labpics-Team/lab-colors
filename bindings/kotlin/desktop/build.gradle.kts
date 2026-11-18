plugins {
    kotlin("jvm") version "2.2.21"
    `maven-publish`
}

group = "pics.lab"
version = providers.gradleProperty("publicationVersion").get()
kotlin { compilerOptions { jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17 } }
sourceSets.main {
    kotlin.srcDir("generated")
    resources.srcDir("nativeResources")
}
dependencies { api("net.java.dev.jna:jna:5.19.1") }
java {
    sourceCompatibility = JavaVersion.VERSION_17
    targetCompatibility = JavaVersion.VERSION_17
    withSourcesJar()
}
publishing {
    publications {
        create<MavenPublication>("desktop") {
            from(components["java"])
            artifactId = "colors-desktop"
            pom {
                name = "Lab Colors for desktop JVM"
                description = "Shared Program runtime with Windows, Linux and macOS native libraries"
                url = "https://github.com/Labpics-Team/lab-colors"
                licenses { license { name = "MIT"; url = "https://opensource.org/license/mit" } }
            }
        }
    }
    repositories { maven { url = uri(providers.gradleProperty("publicationRepository").get()) } }
}

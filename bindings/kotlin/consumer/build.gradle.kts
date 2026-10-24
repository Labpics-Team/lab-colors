plugins {
    id("com.android.application") version "8.13.2"
    kotlin("android") version "2.2.21"
}
android {
    namespace = "pics.lab.colors.consumer"
    compileSdk = 36
    defaultConfig {
        applicationId = "pics.lab.colors.consumer"
        minSdk = 23
        ndk { abiFilters.addAll(providers.gradleProperty("supportedAbis").get().split(",")) }
        targetSdk = 36
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }
    buildTypes {
        getByName("release") {
            isMinifyEnabled = true
            signingConfig = signingConfigs.getByName("debug")
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"))
        }
    }
    testBuildType = providers.gradleProperty("consumerBuildType").getOrElse("debug")
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
}
kotlin { compilerOptions { jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_1_8 } }
dependencies {
    implementation("pics.lab:colors-android:" + providers.gradleProperty("publicationVersion").get())
    androidTestImplementation("androidx.test:runner:1.7.0")
    androidTestImplementation("androidx.test.ext:junit:1.3.0")
}

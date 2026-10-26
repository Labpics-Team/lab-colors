import com.android.build.gradle.internal.tasks.L8DexDesugarLibTask

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
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "instrumentation-rules.pro")
        }
    }
    testBuildType = providers.gradleProperty("consumerBuildType").getOrElse("debug")
    compileOptions {
        isCoreLibraryDesugaringEnabled = true
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
}
kotlin { compilerOptions { jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_1_8 } }

// AGP 8.13.2 shrinks the app/test desugar runtimes independently, although both
// APKs share a class loader. Keep their ABI identical without changing app R8.
// Remove when AGP packages the combined runtime in the tested APK only.
if (providers.gradleProperty("consumerBuildType").getOrElse("debug") == "release") {
    gradle.projectsEvaluated {
        val app = tasks.named<L8DexDesugarLibTask>("l8DexDesugarLibRelease").get()
        val test = tasks.named<L8DexDesugarLibTask>("l8DexDesugarLibReleaseAndroidTest").get()
        // Snapshot original provider sources before replacing either collection:
        // neither provider includes L8 outputs or packaged APKs.
        val inputs = files(app.dexFiles.from.toList(), test.dexFiles.from.toList())
        for (task in listOf(app, test)) {
            task.dexFiles.setFrom(inputs)
            task.keepRulesConfigurations.addAll("-dontobfuscate", "-dontoptimize")
        }
    }
}
dependencies {
    coreLibraryDesugaring("com.android.tools:desugar_jdk_libs:2.1.5")
    implementation("pics.lab:colors-android:" + providers.gradleProperty("publicationVersion").get())
    androidTestImplementation("androidx.test:runner:1.7.0")
    androidTestImplementation("androidx.test.ext:junit:1.3.0")
    // AndroidX Test references these annotations. 2.46 removes the desktop
    // javax.lang.model reference that earlier releases exposed to Android R8.
    androidTestImplementation("com.google.errorprone:error_prone_annotations:2.46.0")
}

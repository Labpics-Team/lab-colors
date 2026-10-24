pluginManagement { repositories { google(); mavenCentral(); gradlePluginPortal() } }
dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        exclusiveContent {
            forRepository { maven { url = uri("installed/repository") } }
            filter { includeGroup("pics.lab") }
        }
        google(); mavenCentral()
    }
}
rootProject.name = "LabColorsConsumer"

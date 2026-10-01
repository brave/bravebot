plugins {
    id("com.android.application")
}

android {
    namespace = "com.brave.bravebot"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.brave.bravebot"
        minSdk = 30
        targetSdk = 35
        versionCode = 1
        versionName = "0.12.0"
        ndk {
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    implementation("androidx.webkit:webkit:1.17.1")
}

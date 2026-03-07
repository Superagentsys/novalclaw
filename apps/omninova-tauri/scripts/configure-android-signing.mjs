import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const {
  ANDROID_KEYSTORE_BASE64,
  ANDROID_KEYSTORE_PASSWORD,
  ANDROID_KEY_ALIAS,
  ANDROID_KEY_PASSWORD,
} = process.env;

if (
  !ANDROID_KEYSTORE_BASE64 ||
  !ANDROID_KEYSTORE_PASSWORD ||
  !ANDROID_KEY_ALIAS ||
  !ANDROID_KEY_PASSWORD
) {
  console.error(
    "Missing Android signing secrets. Expected ANDROID_KEYSTORE_BASE64, ANDROID_KEYSTORE_PASSWORD, ANDROID_KEY_ALIAS, ANDROID_KEY_PASSWORD."
  );
  process.exit(1);
}

const androidRoot = path.resolve("src-tauri/gen/android");
const appDir = path.join(androidRoot, "app");
const gradleKtsPath = path.join(appDir, "build.gradle.kts");
const gradleGroovyPath = path.join(appDir, "build.gradle");
const gradleFilePath = fs.existsSync(gradleKtsPath)
  ? gradleKtsPath
  : fs.existsSync(gradleGroovyPath)
    ? gradleGroovyPath
    : undefined;

if (!gradleFilePath) {
  console.error(
    "Android Gradle file not found. Run `npm run mobile:init:android` before configuring signing."
  );
  process.exit(1);
}

fs.mkdirSync(androidRoot, { recursive: true });

const keystorePath = path.join(androidRoot, "omninova-upload.keystore");
const keystorePropertiesPath = path.join(androidRoot, "keystore.properties");

fs.writeFileSync(
  keystorePath,
  Buffer.from(ANDROID_KEYSTORE_BASE64, "base64")
);

fs.writeFileSync(
  keystorePropertiesPath,
  [
    `storeFile=${keystorePath}`,
    `storePassword=${ANDROID_KEYSTORE_PASSWORD}`,
    `keyAlias=${ANDROID_KEY_ALIAS}`,
    `keyPassword=${ANDROID_KEY_PASSWORD}`,
    "",
  ].join("\n")
);

const marker = "omninova signing config";
let gradleContent = fs.readFileSync(gradleFilePath, "utf8");

if (gradleContent.includes(marker)) {
  console.log(`Android signing already configured: ${gradleFilePath}`);
  process.exit(0);
}

if (gradleFilePath.endsWith(".kts")) {
  const imports = [
    'import java.io.FileInputStream',
    'import java.util.Properties',
  ];

  const missingImports = imports.filter(
    (line) => !gradleContent.includes(line)
  );

  if (missingImports.length > 0) {
    gradleContent = `${missingImports.join("\n")}\n${gradleContent}`;
  }

  gradleContent += `

// omninova signing config
val omninovaKeystorePropertiesFile = rootProject.file("keystore.properties")
val omninovaKeystoreProperties = Properties().apply {
    if (omninovaKeystorePropertiesFile.exists()) {
        load(FileInputStream(omninovaKeystorePropertiesFile))
    }
}

android {
    signingConfigs {
        create("omninovaRelease") {
            if (omninovaKeystorePropertiesFile.exists()) {
                keyAlias = omninovaKeystoreProperties["keyAlias"] as String
                keyPassword = omninovaKeystoreProperties["keyPassword"] as String
                storeFile = file(omninovaKeystoreProperties["storeFile"] as String)
                storePassword = omninovaKeystoreProperties["storePassword"] as String
            }
        }
    }

    buildTypes {
        getByName("release") {
            signingConfig = signingConfigs.getByName("omninovaRelease")
        }
    }
}
`;
} else {
  gradleContent += `

// omninova signing config
def omninovaKeystorePropertiesFile = rootProject.file("keystore.properties")
def omninovaKeystoreProperties = new Properties()
if (omninovaKeystorePropertiesFile.exists()) {
    omninovaKeystoreProperties.load(new FileInputStream(omninovaKeystorePropertiesFile))
}

android {
    signingConfigs {
        omninovaRelease {
            if (omninovaKeystorePropertiesFile.exists()) {
                keyAlias omninovaKeystoreProperties["keyAlias"]
                keyPassword omninovaKeystoreProperties["keyPassword"]
                storeFile file(omninovaKeystoreProperties["storeFile"])
                storePassword omninovaKeystoreProperties["storePassword"]
            }
        }
    }

    buildTypes {
        release {
            signingConfig signingConfigs.omninovaRelease
        }
    }
}
`;
}

fs.writeFileSync(gradleFilePath, gradleContent);
console.log(`Configured Android signing: ${gradleFilePath}`);

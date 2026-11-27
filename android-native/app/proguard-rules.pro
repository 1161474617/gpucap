-dontwarn okhttp3.**
-dontwarn okio.**
-dontwarn org.conscrypt.**
-dontwarn org.bouncycastle.**
-dontwarn org.openjsse.**
-keepattributes *Annotation*, InnerClasses

# 保留 ML Kit / Firebase 组件注册器及无参构造方法（防止 R8 full mode 移除导致扫码 NPE）
-keep class * implements com.google.firebase.components.ComponentRegistrar {
    <init>();
    java.util.List getComponents();
}
-keepnames class com.google.mlkit.**
-keep class com.google.android.libraries.barhopper.** { *; }
-dontwarn com.google.mlkit.**

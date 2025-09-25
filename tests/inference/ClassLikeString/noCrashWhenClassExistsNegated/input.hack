final class A {}

if (!class_exists(nameof A)) {
    new \RuntimeException();
}
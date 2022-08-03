function funB(string $className) : ?ReflectionClass {
    if (class_exists($className)) {
        return new ReflectionClass($className);
    }

    if (interface_exists($className)) {
        return new ReflectionClass($className);
    }

    return null;
}
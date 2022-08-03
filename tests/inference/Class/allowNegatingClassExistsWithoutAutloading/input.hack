function specifyString(string $className): void{
    if (!class_exists($className, false)) {
        return;
    }
    new ReflectionClass($className);
}
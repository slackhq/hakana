$object = new stdClass();
$reflection = new ReflectionClass($object);

foreach ($reflection->getProperties() as $property) {
    $message = $property->getValue($reflection->newInstance());

    if (!$message is string) {
        throw new RuntimeException();
    }
}
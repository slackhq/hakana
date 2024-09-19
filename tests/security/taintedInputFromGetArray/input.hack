function getName(dict<arraykey, mixed> $data) : string {
    return $data["name"] ?? "unknown";
}

$name = getName(HH\global_get('_GET'));

echo $name;
function getName(dict<arraykey, mixed> $data) : string {
    return $data["name"] ?? "unknown";
}

$name = getName($_GET);

echo $name;
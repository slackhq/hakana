function getName() : string {
    return $_GET["name"] ?? "unknown";
}

echo getName();
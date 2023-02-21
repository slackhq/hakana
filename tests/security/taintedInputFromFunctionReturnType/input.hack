function getName() : string {
    $a = $_GET["name"] ?? "unknown";
    return $a;
}

echo getName();
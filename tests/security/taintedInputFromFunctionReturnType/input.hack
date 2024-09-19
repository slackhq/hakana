function getName() : string {
    $a = HH\global_get('_GET')["name"] ?? "unknown";
    return $a;
}

echo getName();
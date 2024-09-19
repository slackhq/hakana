function get_data(): dict<string, string> {
    $name = HH\global_get('_GET')["name"] ?? "unknown";

    $data = dict[];
    $data["name"] = $name;
    return $data;
}

$data = get_data();

echo "<h1>" . $data["name"] . "</h1>";
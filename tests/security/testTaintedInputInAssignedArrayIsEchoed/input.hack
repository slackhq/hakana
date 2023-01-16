function get_data(): dict<string, string> {
    $name = $_GET["name"] ?? "unknown";

    $data = dict[];
    $data["name"] = $name;
    return $data;
}

$data = get_data();

echo "<h1>" . $data["name"] . "</h1>";
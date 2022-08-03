$name = $_GET["name"] ?? "unknown";

$data = [];
$data["name"] = $name;

echo "<h1>" . $data["name"] . "</h1>";
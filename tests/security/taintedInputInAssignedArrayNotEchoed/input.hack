$name = $_GET["name"] ?? "unknown";
$id = (int) $_GET["id"];

$data = [];
$data["name"] = $name;
$data["id"] = $id;

echo "<h1>" . htmlentities($data["name"], \ENT_QUOTES) . "</h1>";
echo "<p>" . $data["id"] . "</p>";
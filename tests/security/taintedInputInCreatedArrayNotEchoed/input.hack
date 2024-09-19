$name = HH\global_get('_GET')["name"] ?? "unknown";
$id = (int) HH\global_get('_GET')["id"];

$data = dict["name" => $name, "id" => $id];

echo "<h1>" . htmlentities($data["name"], \ENT_QUOTES) . "</h1>";
echo "<p>" . $data["id"] . "</p>";
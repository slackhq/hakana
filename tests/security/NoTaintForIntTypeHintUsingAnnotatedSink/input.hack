function fetch(int $id): string {
    return query("SELECT * FROM table WHERE id=" . $id);
}

function query(<<\Hakana\SecurityAnalysis\Sink('Sql')>> string $sql): string {}

$value = $_GET["value"];
$result = fetch($value);
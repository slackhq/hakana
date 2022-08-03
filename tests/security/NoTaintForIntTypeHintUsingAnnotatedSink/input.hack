function fetch(int $id): string {
    return query("SELECT * FROM table WHERE id=" . $id);
}

<<\Hakana\SecurityAnalysis\Specialize>>
function query(<<\Hakana\SecurityAnalysis\Sink('sql')>> string $sql): string {}

$value = $_GET["value"];
$result = fetch($value);
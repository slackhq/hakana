function fetch($id): string
{
    return query("SELECT * FROM table WHERE id=" . (int)$id);
}

<<\Hakana\SecurityAnalysis\SpecializeCall()>>
function query(
    <<\Hakana\SecurityAnalysis\Sink('Sql')>> string $sql
): string {}

$value = $_GET["value"];
$result = fetch($value);
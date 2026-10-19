function make_query(string $name): (string, dict<string, string>) {
    return tuple('name = %name', dict['name' => $name]);
}

function query(
    <<Hakana\SecurityAnalysis\Sink('Sql')>> string $sql,
    dict<string, string> $bind,
): void {}

list($where, $bind) = make_query((string)HH\global_get('_GET')['name']);
query($where, $bind);

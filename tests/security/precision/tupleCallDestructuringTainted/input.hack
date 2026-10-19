function make_query(string $sql): (string, dict<string, string>) {
    return tuple($sql, dict['name' => 'safe']);
}

function query(
    <<Hakana\SecurityAnalysis\Sink('Sql')>> string $sql,
    dict<string, string> $bind,
): void {}

list($where, $bind) = make_query((string)HH\global_get('_GET')['sql']);
query($where, $bind);

function takesString(string $s): void {}

function test(): void {
    $encoded = json_encode(dict["key" => "value"]);
    takesString($encoded);
}

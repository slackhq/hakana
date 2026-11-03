<<Hakana\SecurityAnalysis\ResponseHeader>>
function response_header(string $value, bool $replace = true): void { header($value, $replace); }
<<Hakana\SecurityAnalysis\ResponseOutput>>
function response_output(mixed $value): void { echo $value; }
<<Hakana\SecurityAnalysis\Source('NonUriRequestHeader')>>
function input(): mixed { return null; }
function run(): void {
    $value = input();
    response_header('Content-Type: text/plain');
    response_output($value);
}

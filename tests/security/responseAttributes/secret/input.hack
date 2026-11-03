<<Hakana\SecurityAnalysis\ResponseHeader>>
function response_header(string $value, bool $replace = true): void { header($value, $replace); }
<<Hakana\SecurityAnalysis\ResponseOutput>>
function response_output(string $value): void { echo $value; }
<<Hakana\SecurityAnalysis\Source('SystemSecret')>>
function secret(): string { return 'private'; }
function run(): void {
    $value = secret();
    response_header('Content-Type: text/plain');
    response_output($value);
}

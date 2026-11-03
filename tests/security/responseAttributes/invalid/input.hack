<<Hakana\SecurityAnalysis\ResponseHeader('x')>>
function bad_header(string $value): void {}
<<Hakana\SecurityAnalysis\ResponseOutput>>
function bad_output(): void {}
<<Hakana\SecurityAnalysis\ResponseOutput>>
function returns_value(string $value): string { return $value; }
<<Hakana\SecurityAnalysis\ResponseOutput>>
async function async_output(string $value): Awaitable<void> {}
<<Hakana\SecurityAnalysis\ResponseHeader, Hakana\SecurityAnalysis\ResponseOutput>>
function both(string $value): void {}
function bad_param(<<Hakana\SecurityAnalysis\ResponseOutput>> string $value): void {}

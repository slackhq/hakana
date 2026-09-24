namespace Hakana\SecurityAnalysis;

/**
 * A complete JSON expression encoded for HTML script text.
 * The contract does not remove taint from strings, decoders, or URL consumers.
 */
final class HtmlSafeJson implements \HH\FunctionAttribute, \HH\MethodAttribute {}

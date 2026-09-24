namespace Hakana\SecurityAnalysis;

/**
 * JavaScript sinks declared by or directly inside this function/method have
 * been audited to handle HTML-safe JSON as complete data values.
 * Raw input and values transformed after encoding still require checking.
 * URL, executable-resource and disclosure obligations remain independent.
 */
final class AcceptsHtmlSafeJson implements \HH\FunctionAttribute, \HH\MethodAttribute {}

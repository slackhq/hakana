function load_from_db(string $id): string {
    return $id;
}

class EntityLoader {
    public function loadEntity(
        <<\Hakana\Security\TransformTaint>> string $id,
    ): string {
        return load_from_db($id);
    }
}

function render(
    <<\Hakana\SecurityAnalysis\Sink('Output')>> string $html,
): void {}

function log_message(
    <<\Hakana\SecurityAnalysis\Sink('Logging')>> string $msg,
): void {}

function handleRequest(EntityLoader $loader): void {
    $id = (string) HH\global_get('_GET')['entity_id'];
    $entity = $loader->loadEntity($id);
    render($entity);
    log_message($entity);
}

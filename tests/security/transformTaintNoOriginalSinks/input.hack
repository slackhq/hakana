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

function query(
    <<\Hakana\SecurityAnalysis\Sink('Sql')>> string $sql,
): void {}

function handleRequest(EntityLoader $loader): void {
    $id = (string) HH\global_get('_GET')['entity_id'];
    $entity = $loader->loadEntity($id);
    query($entity);
}

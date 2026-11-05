#!/usr/bin/env python3
"""Entity facts from the jars of an install: which entity events fire for which subjects, which state of an entity mod code touches,
which vanilla entity methods mods change, and the capabilities mods attach to vanilla entity types. No mod code runs.

Called at the end of `mod_replaces.analyse`; `entity-facts.json` (format 2) is written beside `replaces.json`:
  events           {event: {listeners: [{mod, class, method, guards}], subjects: "all" | [guards]}}; a guard is {side, require, exclude, types}
  reads            {class: [group]}: the state groups mod code reads or writes through that receiver class
  writes           [{key, receiver, group, mods}]: vanilla entity writers that mod code calls
  routes           {class: {edge: {key: [{mixin, handler, at, cancellable}]}, body: [key]}}
  overrides        {mod class: [key]}
  caps             {entity type id: [capability field]}; caps_unresolved {mod: calls whose type or capability is not a static field}
  opaque           ["mod: why"] jars that can't be analysed ahead
  hooks            {hook: {reads: [group], writes: [{key, receiver, group}], events: [event class]}}: what NeoForge's own hooks do, from the jar's bytecode
  hooks_missing    [hook key] a HOOKS line the NeoForge jar doesn't have
  unclassified     entity methods mod code calls that GROUPS doesn't name (the tool exits non-zero)
  unresolved_listeners {mod: n}; reflective_entity_access {mod: n}
A key is `class#name(descriptor)`; a bytecode-level guard that can't be proven is `all`.
"""
import collections
import re
import struct
import zipfile

try:
    from . import mod_replaces as mr
except ImportError:
    import mod_replaces as mr

ENTITY = 'net/minecraft/world/entity/Entity'
PLAYER = 'net/minecraft/world/entity/player/Player'
TYPES_CLASS = 'net/minecraft/world/entity/EntityTypes'
TYPE_IDS_CLASS = 'net/minecraft/world/entity/EntityTypeIds'
CAPS_EVENT = 'net/neoforged/neoforge/capabilities/RegisterCapabilitiesEvent'
EVENT_PREFIXES = ('net/neoforged/neoforge/event/tick/Entity', 'net/neoforged/neoforge/client/event/RenderLiving', 'net/neoforged/neoforge/client/event/RenderNameTag')
ENTITY_EVENT = 'net/neoforged/neoforge/event/entity/EntityEvent'
CLIENT_ROUTES = ('net/minecraft/client/renderer/entity/', 'net/minecraft/client/model/')
EDGE_AT = ('HEAD', 'RETURN', 'TAIL')
CALLBACKS = ('org/spongepowered/asm/mixin/injection/callback/CallbackInfo', 'org/spongepowered/asm/mixin/injection/callback/CallbackInfoReturnable')
INJECT = 'Lorg/spongepowered/asm/mixin/injection/Inject;'
REFLECTION = ('java/lang/reflect/', 'java/lang/invoke/MethodHandles')
ALL_GUARD = {'side': 'both', 'require': [], 'exclude': [], 'types': None}

# Opcodes
RETURN, GOTO, IFEQ, IFNE, IF_ACMPEQ, IF_ACMPNE = 0xb1, 0xa7, 0x99, 0x9a, 0xa5, 0xa6
GETSTATIC, PUTSTATIC, GETFIELD = 0xb2, 0xb3, 0xb4
INVOKEVIRTUAL, INVOKESPECIAL, INVOKESTATIC, INVOKEINTERFACE, INVOKEDYNAMIC = 0xb6, 0xb7, 0xb8, 0xb9, 0xba
INSTANCEOF, CHECKCAST, DUP, POP = 0xc1, 0xc0, 0x59, 0x57
LDC, LDC_W = 0x12, 0x13

# The state groups (bit order of `entities.bin`)
STATE_GROUPS = ('identity', 'position', 'motion', 'health', 'effects', 'equipment', 'fire', 'air', 'flags', 'name', 'item', 'riding', 'removal', 'species', 'attributes', 'ai')
UNCARRIED = ('java', 'pure')
WRITER_PREFIXES = ('set', 'add', 'remove', 'hurt', 'kill', 'discard', 'teleport', 'push', 'knockback', 'heal', 'ignite', 'extinguish', 'startRiding', 'stopRiding',
                   'snapTo', 'moveTo', 'absSnapTo', 'lerpTo', 'resetFallDistance', 'clearFire')


# Bytecode

def instructions(code):
    """[(offset, opcode, operand)]: a pool index (getstatic, putstatic, getfield, invoke*, instanceof, checkcast, ldc, new), a local index
    (loads and stores, aload_n and astore_n included), the absolute target of a branch, else None."""
    out, p = [], 0
    while p < len(code):
        op = code[p]
        operand = None
        if op == LDC:
            operand = code[p + 1]
        elif op in (0x13, 0x14) or 0xb2 <= op <= 0xba or op in (0xbb, 0xbd, 0xc0, 0xc1):
            operand = mr.u2(code, p + 1)
        elif 0x99 <= op <= 0xa8 or op in (0xc6, 0xc7):
            operand = p + struct.unpack_from('>h', code, p + 1)[0]
        elif op in (0xc8, 0xc9):
            operand = p + struct.unpack_from('>i', code, p + 1)[0]
        elif 0x15 <= op <= 0x19 or 0x36 <= op <= 0x3a:
            operand = code[p + 1]
        elif 0x1a <= op <= 0x2d:
            operand = (op - 0x1a) % 4
        elif 0x3b <= op <= 0x4e:
            operand = (op - 0x3b) % 4
        elif op == 0xc4:
            operand = mr.u2(code, p + 2)
        out.append((p, op, operand))
        p += mr.op_length(code, p)
    return out


def is_exit(by_offset, target):
    """Whether the instruction at target is a return, or a goto to one."""
    ins = by_offset.get(target)
    if ins and ins[1] == GOTO:
        ins = by_offset.get(ins[2])
    return bool(ins and ins[1] == RETURN)


def is_aload(op):
    return op == 0x19 or 0x2a <= op <= 0x2d


def is_astore(op):
    return op == 0x3a or 0x4b <= op <= 0x4e


def slot_of_last(desc, static):
    """The local slot of the last parameter of a method."""
    slot = 0 if static else 1
    for t in mr.params(desc)[:-1]:
        slot += 2 if t in ('J', 'D') else 1
    return slot


# Entity types of the vanilla jar

def read_library_class(hier, name):
    for jar in hier.jars:
        with zipfile.ZipFile(jar) as z:
            try:
                return z.read(name + '.class')
            except KeyError:
                continue
    return None


def clinit(data):
    info = mr.parse_class(data, code=True)
    method = next((m for m in info['methods'] if m['name'] == '<clinit>' and 'code' in m), None)
    return info['pool'], instructions(method['code']) if method else []


def type_ids(hier):
    """{static field of EntityTypes: registry id}, from the clinit of EntityTypes and EntityTypeIds, never by changing case."""
    holder, ids = read_library_class(hier, TYPES_CLASS), read_library_class(hier, TYPE_IDS_CLASS)
    if holder is None:
        return {}
    names = {}
    if ids is not None:
        pool, ins = clinit(ids)
        last = None
        for _, op, operand in ins:
            if op in (LDC, LDC_W) and pool.e[operand] and pool.e[operand][0] == 8:
                last = pool.utf(pool.e[operand][1])
            elif op == PUTSTATIC and last is not None:
                names[pool.ref(operand)[1]] = last
                last = None
    pool, ins = clinit(holder)
    found, key, string = {}, None, None
    for _, op, operand in ins:
        if op == GETSTATIC and pool.ref(operand)[0] == TYPE_IDS_CLASS:
            key = pool.ref(operand)[1]
        elif op in (LDC, LDC_W) and pool.e[operand] and pool.e[operand][0] == 8:
            string = pool.utf(pool.e[operand][1])
        elif op == PUTSTATIC and pool.ref(operand)[0] == TYPES_CLASS:
            name = names.get(key) if key else string
            if name:
                found[pool.ref(operand)[1]] = name if ':' in name else 'minecraft:' + name
            key = string = None
    return found


# Listener guards

def walk_guards(code, pool, locals_, ids):
    """The guards a listener's prefix proves: a symbolic walk that stops at the first thing it can't follow (see SPEC-0-analysis)."""
    ins = instructions(code)
    by_offset = {o: (o, op, x) for o, op, x in ins}
    side, require, exclude, types = 'both', [], [], None
    stack, local = [], dict(locals_)
    for _, op, operand in ins:
        if is_aload(op):
            stack.append(local.get(operand))
        elif is_astore(op):
            local[operand] = stack.pop() if stack else None
        elif op == DUP:
            if not stack:
                break
            stack.append(stack[-1])
        elif op == POP:
            if not stack:
                break
            stack.pop()
        elif op == CHECKCAST:
            if not stack:
                break
        elif op == INSTANCEOF:
            value = stack.pop() if stack else None
            if value != 'ENT':
                break
            stack.append(('INST', pool.cls(operand)))
        elif op == GETSTATIC:
            owner, name, _ = pool.ref(operand)
            if owner != TYPES_CLASS or name not in ids:
                break
            stack.append(('T', ids[name]))
        elif op in (INVOKEVIRTUAL, INVOKEINTERFACE):
            _, name, desc = pool.ref(operand)
            receiver = stack.pop() if stack else None
            if name == 'getEntity' and desc.startswith('()') and receiver == 'EV':
                stack.append('ENT')
            elif name == 'level' and desc.startswith('()') and receiver == 'ENT' or name == 'getLevel' and desc.startswith('()') and receiver == 'EV':
                stack.append('LVL')
            elif name == 'isClientSide' and desc == '()Z' and receiver == 'LVL':
                stack.append('CS')
            elif name == 'getType' and desc.startswith('()') and receiver == 'ENT':
                stack.append('TYPE')
            else:
                break
        elif op in (IFEQ, IFNE):
            value = stack.pop() if stack else None
            if not is_exit(by_offset, operand):
                break
            if value == 'CS':
                side = 'server' if op == IFNE else 'client'
            elif isinstance(value, tuple) and value[0] == 'INST':
                (require if op == IFEQ else exclude).append(value[1])
            else:
                break
        elif op == IF_ACMPNE:
            pair = [stack.pop() if stack else None, stack.pop() if stack else None]
            known = [v for v in pair if isinstance(v, tuple) and v[0] == 'T']
            if not is_exit(by_offset, operand) or len(known) != 1 or 'TYPE' not in pair:
                break
            types = [known[0][1]] if types is None else [t for t in types if t == known[0][1]]
        else:
            break
    return {'side': side, 'require': sorted(set(require)), 'exclude': sorted(set(exclude)), 'types': types}


def listener_guards(infos, impl, ids):
    """The guards of one listener implementation, or None when it can't be resolved."""
    if not impl:
        return None
    cls, name, desc, static = impl
    info = infos(cls)
    method = next((m for m in info['methods'] if m['name'] == name and m['desc'] == desc and 'code' in m), None) if info else None
    if method is None or not mr.params(desc):
        return None
    return walk_guards(method['code'], info['pool'], {slot_of_last(desc, static): 'EV'}, ids)


def subjects(listeners):
    """The event-level filter: `all`, or the distinct guards, one alternative per listener."""
    guards = [l['guards'] for l in listeners]
    if any(g == ALL_GUARD for g in guards):
        return 'all'
    unique = []
    for g in guards:
        if g not in unique:
            unique.append(g)
    return unique


# State groups of the methods mod code calls

GROUP_NAMES = {
    'identity': 'getId setId getUUID getType typeHolder hashCode is equals asLivingEntity setUUID',
    'name': 'getName getDisplayName hasCustomName getCustomName setCustomName setCustomNameVisible entityTags',
    'position': ('blockPosition getBoundingBox getBbHeight getBbWidth getBlockY getEyePosition getLookAngle getViewVector getLightProbePosition getEyeY getOnPos getX getY getZ '
                 'getXRot getYRot getYHeadRot oldPosition position setPos setXRot setYRot setYBodyRot setYHeadRot setOldPos setOldPosAndRot snapTo absSnapTo teleport '
                 'teleportTo distanceTo distanceToSqr makeBoundingBox getDimensions getDirection BlockAttachedEntity#getPos canCoexist getEyeHeight getVisualRotationYInDegrees setInitialPos setStartPos syncPacketPositionCodec'),
    'motion': 'getDeltaMovement setDeltaMovement move knockback shootFromRotation lerpMotion onInsideBubbleColumn resetFallDistance makeStuckInBlock',
    'health': 'thunderHit getHealth setHealth getMaxHealth isAlive getAbsorptionAmount hurt hurtServer actuallyHurt isInvulnerableTo isInvulnerableToBase getInvulnerableTime isInvulnerable markHurt getLastHurtByMob heal',
    'effects': 'getActiveEffects addEffect getEffect hasEffect removeAllEffects',
    'air': 'getAirSupply setAirSupply getMaxAirSupply',
    'equipment': ('getItemBySlot hasItemInSlot getMainHandItem getOffhandItem getItemBlockingWith getUsedItemHand releaseUsingItem setItemSlot getBodyArmorItem getMainArm getUseItem getUseItemRemainingTicks stopUsingItem setItemInHand setDropChance getItemInHand'),
    'fire': 'isOnFire getRemainingFireTicks setRemainingFireTicks igniteForSeconds igniteForTicks fireImmune',
    'flags': ('onGround getPose isInWater isInvisible isInvisibleTo isSpectator isPickable isFullyFrozen setSilent getFluidInteraction shouldTakeDrowningDamage '
              'canBeRiddenUnderFluidType canDrownInFluidType isCrouching isShiftKeyDown isSprinting isNoGravity setNoGravity isInLava isFallFlying setOnGround'),
    'item': 'ItemEntity#getItem ItemFrame#getItem setItem hasPickUpDelay setPickUpDelay setNoPickUpDelay setDefaultPickUpDelay ItemEntity#setTarget makeFakeItem getFrameItemStack dropItem ItemFrame#getRotation ItemFrame#setRotation setThrower setUnlimitedLifetime',
    'riding': 'getVehicle getPassengers getIndirectPassengers hasPassenger isPassenger getControllingPassenger startRiding stopRiding getRootVehicle getControlledVehicle isVehicle ejectPassengers onPassengerTurned showVehicleHealth dropLeash getLeashHolder isLeashed setLeashedTo',
    'removal': 'isRemoved remove discard setRemoved kill',
    'species': ('Entity#get Sheep#getColor Sheep#setColor isSheared isBaby isAgeLocked getAge getTicksLeftUntilAdult isInLove isSitting isInSittingPose isAngry isCamelSitting isConverting '
                'isCreepy isPlayingDead isPowered isSaddled isScreamingGoat hasLeftHorn hasRightHorn hasNectar getCarriedBlock getMarkings getState getStrength getSwelling '
                'getVariant getVillagerData getWeatherState getOffers getSize isSleeping getDescription getSpawner setBaseDamage setFuse getFuse setBlockState getBlockState '
                'getItemStack getOwner setOwner setDamage setVisualOnly ExperienceOrb#getValue ExperienceOrb#setValue getEntityData AbstractMinecartContainer#getItem getContainerLootTable getContainerLootTableSeed setLootTable getStingerCount setStingerCount getSpawnType getOwnerReference isOrderedToSit setTamed isDancing setDancing startConverting stopSleeping getCurrentRaid'),
    'attributes': 'getAttribute getAttributeBaseValue getAttributeValue getAttributes getArmorValue',
    'ai': 'setNoActionTime getNavigation getBrain getLookControl getPathfindingMalus setPathfindingMalus getMaxHeadXRot',
    'all': 'save load',
    'java': 'getData setData getCapability syncData hasData removeData getPersistentData getExistingDataOrNull',
    'pure': ('level registryAccess damageSources getRandom gameEvent playSound canTeleport canUsePortal canHitEntity updateFluidInteraction getLootTable getPickResult '
             'getParts isMultipartEntity shearOffAllLeashConnections tick baseTick explode onHit onHitBlock registerGoals readAdditionalSaveData addAdditionalSaveData '
             'defineSynchedData populateDefaultEquipmentSlots triggerOnDeathMobEffects sendEffectToPassengers refreshDirtyAttributes take spawnLingeringCloud getArrow '
             'getSoundSource onAddedToLevel onRemovedFromLevel onClientRemoval Container#getContainerSize MinecartChest#getContainerSize setChanged getPistonPushReaction spawnAtLocation startSeenByPlayer getRemoveItemSound getRotateItemSound onEquippedItemBroken playAmbientSound'),
}
GROUPS = {name: group for group, names in GROUP_NAMES.items() for name in names.split()}
EXTRA_WRITERS = ('makeStuckInBlock', 'ejectPassengers', 'dropItem', 'dropLeash', 'load', 'lerpMotion', 'markHurt', 'onInsideBubbleColumn', 'onPassengerTurned', 'startConverting',
                 'stopSleeping', 'stopUsingItem', 'syncPacketPositionCodec', 'move', 'shootFromRotation', 'releaseUsingItem', 'makeFakeItem', 'actuallyHurt', 'thunderHit')
NOT_WRITERS = ('setChanged', 'addAdditionalSaveData', 'removeWhenFarAway', 'hurtServerCheck')


def carried(group):
    """The state groups a method of this group needs in a view: `all` (whole-entity save and load) needs every one."""
    return STATE_GROUPS if group == 'all' else (group,)


def classify(owner, name):
    """(group, write) of an entity method, or None when GROUPS doesn't name it."""
    group = GROUPS.get(f'{owner.rsplit("/", 1)[-1]}#{name}') or GROUPS.get(name)
    if group is None:
        return None
    return group, (name.startswith(WRITER_PREFIXES) or name in EXTRA_WRITERS) and name not in NOT_WRITERS


def entity_receiver(hier, owner):
    return bool(owner) and hier.is_a(owner, ENTITY) and not hier.is_a(owner, PLAYER)


def entity_key(hier, owner, name, desc):
    """`class#name(desc)` of the vanilla entity method a call reaches, else None: a mod's own method, or a static one (no entity state)."""
    declarer = hier.declarer(owner, name, desc)
    if not mr.is_vanilla(declarer) and not mr.is_extension(declarer):
        if not mr.is_vanilla(owner):
            return None
        declarer = owner
    if any(m[0] == name and m[1] == desc and m[2] & mr.ACC_STATIC for m in hier.methods_of(declarer)):
        return None
    return f'{declarer}#{name}{desc}'


# Capabilities

def producers_of_call(pool, ins, at):
    """Walks back from the invoke at ins[at], also through calls and invokedynamic; the producers of its receiver and arguments, deepest first."""
    _, op, operand = ins[at]
    desc = pool.ref(operand)[2]
    args = len(mr.params(desc))
    needs, found = ['top'] * (args + 1), []
    i = at - 1
    while needs and i >= 0:
        _, o, x = ins[i]
        i -= 1
        if is_aload(o) or o in (GETSTATIC, LDC, LDC_W, 0x01):
            pops, pushes = 0, 1
        elif o in (CHECKCAST, GETFIELD):
            pops, pushes = 1, 1
        elif o == INVOKEDYNAMIC:
            _, _, nt = pool.e[x]
            pops, pushes = len(mr.params(pool.nat(nt)[1])), 1
        elif o in (INVOKEVIRTUAL, INVOKESTATIC, INVOKEINTERFACE, INVOKESPECIAL):
            cdesc = pool.ref(x)[2]
            pops = len(mr.params(cdesc)) + (0 if o == INVOKESTATIC else 1)
            pushes = 0 if cdesc.endswith(')V') else 1
        else:
            return None
        if not pushes:
            return None
        tag = needs.pop()
        if tag == 'top':
            found.append(ins[i + 1])
        needs += ['nested'] * pops
    return found[::-1] if not needs else None


def capability_registrations(method, pool, ids):
    """[(type id, 'Owner.FIELD') | None] for each registerEntity call of a method: None when the type or the capability isn't a static field."""
    ins = instructions(method['code'])
    out = []
    for at, (_, op, operand) in enumerate(ins):
        if op not in (INVOKEVIRTUAL, INVOKEINTERFACE):
            continue
        owner, name, desc = pool.ref(operand)
        if owner != CAPS_EVENT or name != 'registerEntity':
            continue
        args = producers_of_call(pool, ins, at)
        if not args or len(args) < 3:
            out.append(None)
            continue
        cap, kind = args[1], args[2]
        if cap[1] != GETSTATIC or kind[1] != GETSTATIC:
            out.append(None)
            continue
        kowner, kfield, _ = pool.ref(kind[2])
        cowner, cfield, _ = pool.ref(cap[2])
        out.append((ids[kfield], f'{cowner}.{cfield}') if kowner == TYPES_CLASS and kfield in ids else None)
    return out


# Routes

def library_target(hier, owner, name, desc, handler_args):
    """The descriptors a mixin target stands for: its own, else the library overloads of that name (the one the handler fits when it fits one)."""
    if desc:
        return [desc]
    overloads = [m[1] for m in hier.methods_of(owner) if m[0] == name]
    fitting = [d for d in overloads if handler_args is not None and mr.params(d) == handler_args]
    return fitting or overloads or [None]


def handler_method(archives, mod, mixin, handler):
    """The parsed methods named handler in the mixin class of that mod's jars."""
    entry = mixin.replace('.', '/') + '.class'
    for m, _, archive, _ in archives:
        if m == mod and entry in archive.namelist():
            info = mr.parse_class(archive.read(entry), annotations=True)
            return [x for x in info['methods'] if x['name'] == handler]
    return []


def route_facts(archives, hier, methods):
    """routes[class] = {edge: {key: [handler]}, body: [key], body_conditional: {key: [handler]}}.

    A handler of a config-plugin mixin is `conditional`; a body key whose changes are all conditional lists them in body_conditional, for boot to check."""
    routes = collections.defaultdict(lambda: {'edge': collections.defaultdict(list), 'body': {}})
    for key, row in methods.items():
        owner, _, name = key.partition('#')
        if not (hier.is_a(owner, ENTITY) or owner.startswith(CLIENT_ROUTES)) or not row['vanilla'] or name == '<unknown>':
            continue
        for change in row['changes']:
            if change['mechanism'] != 'mixin':
                continue
            mixin, _, handler = change['handler'].rpartition('.')
            found = handler_method(archives, change['mod'], mixin, handler)
            args = None
            edge = change['kind'] == 'inject' and change['at'] and all(a in EDGE_AT for a in change['at']) and len(found) == 1
            if edge:
                hp = mr.params(found[0]['desc'])
                edge = bool(hp) and hp[-1] in CALLBACKS
                args = hp[:-1] if edge else None
            keys = [f'{owner}#{name}{d or ""}' for d in library_target(hier, owner, name, change['desc'], args)]
            conditional = bool(change.get('conditional'))
            if edge:
                cancellable = bool((found[0]['annos'].get(INJECT) or {}).get('cancellable'))
                for at in change['at']:
                    routes[owner]['edge'][keys[0]].append({'mixin': mixin, 'handler': handler, 'at': at, 'cancellable': cancellable, 'conditional': conditional})
            else:
                for k in keys:
                    routes[owner]['body'].setdefault(k, []).append({'mixin': mixin, 'handler': handler, 'conditional': conditional})
    out = {}
    for cls, r in sorted(routes.items()):
        body = r['body']
        out[cls] = {'edge': dict(sorted(r['edge'].items())), 'body': sorted(body),
                    'body_conditional': {k: v for k, v in sorted(body.items()) if all(h['conditional'] for h in v)}}
    return out


# NeoForge's hooks

HOOKS = {
    'entity_tick_pre': 'net/neoforged/neoforge/event/EventHooks#fireEntityTickPre(Lnet/minecraft/world/entity/Entity;)Lnet/neoforged/neoforge/event/tick/EntityTickEvent$Pre;',
    'entity_tick_post': 'net/neoforged/neoforge/event/EventHooks#fireEntityTickPost(Lnet/minecraft/world/entity/Entity;)V',
    'mob_despawn': 'net/neoforged/neoforge/event/EventHooks#checkMobDespawn(Lnet/minecraft/world/entity/Mob;)Z',
    'living_breathe': 'net/neoforged/neoforge/common/CommonHooks#onLivingBreathe(Lnet/minecraft/world/entity/LivingEntity;Lnet/minecraft/server/level/ServerLevel;II)V',
    'start_tracking': 'net/neoforged/neoforge/event/EventHooks#onStartEntityTracking(Lnet/minecraft/world/entity/Entity;Lnet/minecraft/world/entity/player/Player;)V',
    'stop_tracking': 'net/neoforged/neoforge/event/EventHooks#onStopEntityTracking(Lnet/minecraft/world/entity/Entity;Lnet/minecraft/world/entity/player/Player;)V',
}
NEW = 0xbb
FOLLOWED = 'net/neoforged/'
NOT_FOLLOWED = 'net/neoforged/bus/'


def hook_closure(hier, key, events=None, unclassified=None):
    """{reads, writes, events} of one HOOKS line, or None when the jar has no such method: the entity calls of the hook and of every NeoForge method it reaches.

    `events` is the set of event classes a `new` may name (default: event_classes); an unmapped entity method goes into `unclassified` as `Owner#name`."""
    events = set(event_classes(hier)) if events is None else events
    unclassified = {} if unclassified is None else unclassified
    cache, seen = {}, set()
    reads, writes, posted = set(), {}, set()

    def parsed(cls):
        if cls not in cache:
            data = read_library_class(hier, cls)
            try:
                cache[cls] = mr.parse_class(data, code=True) if data else None
            except (ValueError, IndexError, struct.error):
                cache[cls] = None
        return cache[cls]

    def find(owner, name, desc):
        for cls in [owner] + hier.ancestors(owner):
            if not cls.startswith(FOLLOWED) or cls.startswith(NOT_FOLLOWED):
                continue
            info = parsed(cls)
            method = next((m for m in info['methods'] if m['name'] == name and m['desc'] == desc and 'code' in m), None) if info else None
            if method is not None:
                return info, method
        return None

    def visit(owner, name, desc):
        if (owner, name, desc) in seen:
            return False
        seen.add((owner, name, desc))
        found = find(owner, name, desc)
        if found is None:
            return False
        info, method = found
        pool = info['pool']
        for _, op, operand in instructions(method['code']):
            if op == NEW:
                if pool.cls(operand) in events:
                    posted.add(pool.cls(operand))
            elif op == INVOKEDYNAMIC:
                impl = mr.lambda_impl(info, pool.e[operand][1])
                if impl:
                    visit(*impl[:3])
            elif op in (INVOKEVIRTUAL, INVOKESPECIAL, INVOKESTATIC, INVOKEINTERFACE):
                callee, cname, cdesc = pool.ref(operand)
                if entity_receiver(hier, callee):
                    ekey = entity_key(hier, callee, cname, cdesc)
                    if ekey and not cname.startswith('<'):
                        hit = classify(ekey.split('#')[0], cname)
                        if hit is None:
                            unclassified[f'{ekey.split("#")[0]}#{cname}'] = 'neoforge'
                        else:
                            if hit[0] not in UNCARRIED:
                                reads.update(carried(hit[0]))
                            if hit[1]:
                                writes[(ekey, callee, hit[0])] = True
                elif callee.startswith(FOLLOWED) and not callee.startswith(NOT_FOLLOWED):
                    visit(callee, cname, cdesc)
        return True

    owner, _, rest = key.partition('#')
    name, _, tail = rest.partition('(')
    if not visit(owner, name, '(' + tail):
        return None
    return {'reads': sorted(reads, key=STATE_GROUPS.index), 'writes': [{'key': k, 'receiver': r, 'group': g} for k, r, g in sorted(writes)], 'events': sorted(posted)}


# The analysis

def loader_archive(hier):
    """NeoForge's own jar as one more archive, and its classes that register listeners: its listeners are in the bus at boot like the mods'."""
    for jar in hier.jars:
        if jar.name.startswith('neoforge-') and jar.name.endswith('-universal.jar'):
            archive = zipfile.ZipFile(jar)
            found = []
            for entry in archive.namelist():
                if entry.endswith('.class') and not entry.startswith('META-INF/'):
                    data = archive.read(entry)
                    if b'SubscribeEvent' in data or b'addListener' in data:
                        found.append((entry, data))
            return ('neoforge', 'neoforge', archive, {}), found
    return None, []


def event_classes(hier):
    """Every entity event class the libraries and the mods define."""
    return [c for c in hier.heads if not mr.is_vanilla(c) and (c.startswith(EVENT_PREFIXES) or hier.is_a(c, ENTITY_EVENT))]


def inherit(events, hier, candidates):
    """A listener of an event also hears its subclasses: copy it into every entity event class below it."""
    for event in list(events):
        for below in candidates:
            if below != event and hier.is_a(below, event):
                events.setdefault(below, {'listeners': []})['listeners'] += [dict(l) for l in events[event]['listeners']]


def analyse(classes, archives, hier, entries, result):
    """`entity-facts.json` for the classes `mod_replaces.analyse` listed; `entries` are the mixin handlers."""
    ids = type_ids(hier)
    mods_n = len(archives)
    loader, loader_found = loader_archive(hier)
    if loader:
        archives = list(archives) + [loader]
        classes = list(classes) + [(mods_n, entry, mr.parse_class(data, members=False)['name'], True, True) for entry, data in loader_found]
    cache = {}
    by_name = {name: (ai, entry) for ai, entry, name, _, _ in classes}

    def infos(name):
        if name not in cache:
            ref = by_name.get(name)
            try:
                cache[name] = mr.parse_class(archives[ref[0]][2].read(ref[1]), annotations=True, code=True) if ref else None
            except (ValueError, IndexError, struct.error):
                cache[name] = None
        return cache[name]

    events = collections.defaultdict(lambda: {'listeners': []})
    unresolved = collections.Counter()
    reads = collections.defaultdict(set)
    writers = {}
    unclassified = collections.defaultdict(set)
    caps = collections.defaultdict(list)
    caps_unresolved = collections.Counter()
    reflective = collections.Counter()
    overrides = collections.defaultdict(set)
    for ai, entry, name, marked, _ in classes:
        mod = archives[ai][0]
        raw = archives[ai][2].read(entry)
        listening_only = ai >= mods_n
        entity_class = hier.vanilla_ancestors(name) and hier.is_a(name, ENTITY)
        if not (marked or entity_class or b'net/minecraft/world/entity/' in raw or CAPS_EVENT.encode() in raw):
            continue
        info = infos(name)
        if info is None:
            continue
        if marked:
            for event, via, method, _, _, _, impl in mr.class_listeners(info, hier):
                if event is None or not (event.startswith(EVENT_PREFIXES) or hier.is_a(event, ENTITY_EVENT)):
                    continue
                guards = listener_guards(infos, impl, ids)
                if guards is None:
                    unresolved[mod] += 1
                    guards = ALL_GUARD
                events[event]['listeners'].append({'mod': mod, 'class': name.replace('/', '.'), 'method': method, 'guards': guards})
        if listening_only:
            continue
        touches_entity = False
        for m in info['methods']:
            for ev in m['events']:
                if ev[0] in ('call', 'static') and ev[1] and hier.is_a(ev[1], ENTITY):
                    touches_entity = True
                if ev[0] != 'call' or not entity_receiver(hier, ev[1]):
                    continue
                _, owner, mname, desc = ev
                key = entity_key(hier, owner, mname, desc)
                if key and not mname.startswith('<'):
                    hit = classify(key.split('#')[0], mname)
                    if hit is None:
                        unclassified[f'{key.split("#")[0]}#{mname}'].add(mod)
                    else:
                        group, write = hit
                        if group not in UNCARRIED:
                            reads[owner].update(carried(group))
                        if write:
                            writers.setdefault((key, owner, group), set()).add(mod)
            if 'code' in m and CAPS_EVENT.encode() in raw:
                for reg in capability_registrations(m, info['pool'], ids):
                    if reg is None:
                        caps_unresolved[mod] += 1
                    else:
                        caps[reg[0]].append(reg[1])
        if touches_entity and any(ev[0] == 'call' and ev[1] and ev[1].startswith(REFLECTION) for m in info['methods'] for ev in m['events']):
            reflective[mod] += 1
        if hier.vanilla_ancestors(name):
            for (key, desc), _ in mr.override_records(info, mod, hier).items():
                owner = key.partition('#')[0]
                if hier.is_a(owner, ENTITY) or owner.startswith(CLIENT_ROUTES):
                    overrides[name].add(f'{key}{desc}')
    candidates = event_classes(hier)
    hooks, hooks_missing = {}, []
    for hook, line in HOOKS.items():
        found = {}
        closure = hook_closure(hier, line, set(candidates), found)
        for k, who in found.items():
            unclassified[k].add(who)
        if closure is None:
            hooks_missing.append(line)
            continue
        hooks[hook] = closure
        for w in closure['writes']:
            writers.setdefault((w['key'], w['receiver'], w['group']), set()).add('neoforge')
    inherit(events, hier, candidates)
    mods = result.get('mods', {})
    for event, row in events.items():
        row['subjects'] = subjects(row['listeners'])
        row['listeners'].sort(key=lambda l: (l['mod'], l['class'], l['method']))
    unresolved.update({m: r['unresolved_listeners'] for m, r in mods.items() if r.get('unresolved_listeners')})
    return {'format': 2,
            'event_root': ENTITY_EVENT,
            'event_prefixes': list(EVENT_PREFIXES),
            'event_supers': {c: [a for a in hier.ancestors(c) if a in candidates] for c in sorted(candidates) if any(a in candidates for a in hier.ancestors(c))},
            'events': dict(sorted(events.items())),
            'reads': {c: sorted(g) for c, g in sorted(reads.items())},
            'writes': [{'key': k, 'receiver': r, 'group': g, 'mods': sorted(m)} for (k, r, g), m in sorted(writers.items())],
            'hooks': hooks,
            'hooks_missing': hooks_missing,
            'routes': route_facts(archives, hier, result.get('methods', {})),
            'overrides': {c: sorted(k) for c, k in sorted(overrides.items())},
            'caps': {t: sorted(set(c)) for t, c in sorted(caps.items())},
            'caps_unresolved': dict(sorted(caps_unresolved.items())),
            'opaque': sorted(f'{m}: {why}' for m, r in mods.items() for why in r.get('opaque', [])),
            'unclassified': sorted(f'{k} ({", ".join(sorted(m))})' for k, m in unclassified.items()),
            'unresolved_listeners': dict(sorted(unresolved.items())),
            'reflective_entity_access': dict(sorted(reflective.items()))}

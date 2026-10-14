import { test } from "node:test";
import assert from "node:assert/strict";

import { attachOutputBindingSet, LeaseConflictError } from "../sink.js";

function makeFakeElement(initialProps = {}, initialPriorities = {}) {
  const props = new Map(Object.entries(initialProps));
  const priorities = new Map(Object.entries(initialPriorities));
  const mutations = [];
  return {
    props,
    priorities,
    mutations,
    style: {
      get length() {
        return props.size;
      },
      item(i) {
        return [...props.keys()][i] ?? null;
      },
      getPropertyValue(k) {
        return props.get(k) ?? "";
      },
      getPropertyPriority(k) {
        return priorities.get(k) ?? "";
      },
      setProperty(k, v, priority = "") {
        mutations.push(["set", k, String(v)]);
        if (String(v) === "") {
          props.delete(k);
          priorities.delete(k);
        } else {
          props.set(k, String(v));
          if (priority) priorities.set(k, priority);
          else priorities.delete(k);
        }
      },
      removeProperty(k) {
        mutations.push(["remove", k]);
        props.delete(k);
        priorities.delete(k);
      },
    },
  };
}

test("R1: Disjoint controllers on same element do not purge each other's keys", () => {
  const element = makeFakeElement({ "--lab-role-b": "#222222" });
  const leaseA = attachOutputBindingSet(element, new Set(["--lab-role-a"]));
  const leaseB = attachOutputBindingSet(element, new Set(["--lab-role-b"]));

  leaseA.commit({ "--lab-role-a": "#111111" });

  assert.equal(
    element.props.get("--lab-role-b"),
    "#222222",
    "Controller B's key must survive Controller A's commit"
  );
  assert.equal(
    element.props.get("--lab-role-a"),
    "#111111",
    "Controller A's key must be committed"
  );
});

test("R2: User-authored --lab-* property is never removed by sink commit", () => {
  const element = makeFakeElement({ "--lab-user-custom": "#ffffff" });
  const leaseA = attachOutputBindingSet(element, new Set(["--lab-primary"]));

  leaseA.commit({ "--lab-primary": "#123456" });

  assert.equal(
    element.props.get("--lab-user-custom"),
    "#ffffff",
    "User-authored property must not be removed by sink commit"
  );
  assert.equal(
    element.props.get("--lab-primary"),
    "#123456",
    "Controller A's key must be committed"
  );
});

test("R3: Dispose revokes only owned keys, leaving other controllers and user properties intact", () => {
  const element = makeFakeElement({ "--lab-user": "#ffffff" });
  const leaseA = attachOutputBindingSet(element, new Set(["--lab-a"]));
  const leaseB = attachOutputBindingSet(element, new Set(["--lab-b"]));

  leaseA.commit({ "--lab-a": "#aaaaaa" });
  leaseB.commit({ "--lab-b": "#bbbbbb" });

  leaseA.revoke();

  assert.equal(element.props.has("--lab-a"), false, "Controller A's key must be removed");
  assert.equal(
    element.props.get("--lab-b"),
    "#bbbbbb",
    "Controller B's key must remain intact"
  );
  assert.equal(
    element.props.get("--lab-user"),
    "#ffffff",
    "User property must remain intact"
  );
});

test("R4: Overlapping key attach throws LeaseConflictError and leaves existing lease functional", () => {
  const element = makeFakeElement();
  const leaseA = attachOutputBindingSet(
    element,
    new Set(["--lab-primary", "--lab-bg"])
  );

  assert.throws(
    () => {
      attachOutputBindingSet(element, new Set(["--lab-primary", "--lab-accent"]));
    },
    (err) => {
      return (
        err instanceof LeaseConflictError &&
        err.code === "output_lease_conflict"
      );
    },
    "Must throw LeaseConflictError when attempting overlapping key attach"
  );

  leaseA.commit({ "--lab-primary": "#111111", "--lab-bg": "#222222" });
  assert.equal(element.props.get("--lab-primary"), "#111111");
  assert.equal(element.props.get("--lab-bg"), "#222222");
});

test("R5: Stale async completion after dispose+reattach cannot mutate or delete new generation keys", () => {
  const element = makeFakeElement();
  const leaseA = attachOutputBindingSet(element, new Set(["--lab-a"]));
  leaseA.revoke();

  const leaseB = attachOutputBindingSet(element, new Set(["--lab-a"]));
  leaseB.commit({ "--lab-a": "#gen2" });

  // Stale leaseA attempts commit
  leaseA.commit({ "--lab-a": "#gen1" });
  assert.equal(
    element.props.get("--lab-a"),
    "#gen2",
    "Stale generation commit must be ignored"
  );

  // Stale leaseA attempts revoke
  leaseA.revoke();
  assert.equal(
    element.props.get("--lab-a"),
    "#gen2",
    "Stale generation revoke must be ignored"
  );
});

test("R6: setProperty failure during commit causes full transactional rollback", () => {
  const element = makeFakeElement({ "--lab-primary": "#initial" });
  const lease = attachOutputBindingSet(
    element,
    new Set(["--lab-primary", "--lab-fail"])
  );

  const origSetProperty = element.style.setProperty;
  element.style.setProperty = function (k, v) {
    if (k === "--lab-fail") {
      throw new Error("DOM write failure");
    }
    return origSetProperty.call(this, k, v);
  };

  assert.throws(() => {
    lease.commit({ "--lab-primary": "#next", "--lab-fail": "#invalid" });
  });

  assert.equal(
    element.props.get("--lab-primary"),
    "#initial",
    "Pre-commit value must be restored on write failure"
  );
  assert.equal(
    element.props.has("--lab-fail"),
    false,
    "Failing property must not be left set"
  );
});

test("R7: Reentrant commit during write loop is rejected by reentrancy lock", () => {
  const element = makeFakeElement();
  const lease = attachOutputBindingSet(
    element,
    new Set(["--lab-trigger", "--lab-other"])
  );

  let reentrantError = null;
  const origSetProperty = element.style.setProperty;
  element.style.setProperty = function (k, v) {
    if (k === "--lab-trigger") {
      try {
        lease.commit({ "--lab-trigger": "#reentrant", "--lab-other": "#reentrant" });
      } catch (err) {
        reentrantError = err;
      }
    }
    return origSetProperty.call(this, k, v);
  };

  lease.commit({ "--lab-trigger": "#outer", "--lab-other": "#outer" });

  assert.ok(
    reentrantError,
    "Reentrant commit must be blocked and throw an error"
  );
  assert.match(
    reentrantError.message,
    /reentrant|lock|writing/i,
    "Error message must indicate reentrancy lock"
  );
});

test("R8: Mid-batch signal cancellation triggers complete rollback", () => {
  const element = makeFakeElement({
    "--lab-a": "#origA",
    "--lab-b": "#origB",
  });
  const lease = attachOutputBindingSet(element, new Set(["--lab-a", "--lab-b"]));

  const ac = new AbortController();
  const origSetProperty = element.style.setProperty;
  element.style.setProperty = function (k, v) {
    if (k === "--lab-a") {
      ac.abort(new Error("Mid-batch cancellation"));
    }
    return origSetProperty.call(this, k, v);
  };

  assert.throws(() => {
    lease.commit(
      { "--lab-a": "#newA", "--lab-b": "#newB" },
      { signal: ac.signal }
    );
  });

  assert.equal(
    element.props.get("--lab-a"),
    "#origA",
    "--lab-a must roll back to original"
  );
  assert.equal(
    element.props.get("--lab-b"),
    "#origB",
    "--lab-b must roll back to original"
  );
});

test("R9: Reentrant revoke during commit produces deterministic state", () => {
  const element = makeFakeElement();
  const lease = attachOutputBindingSet(element, new Set(["--lab-a", "--lab-b"]));

  const origSetProperty = element.style.setProperty;
  element.style.setProperty = function (k, v) {
    if (k === "--lab-a") {
      lease.revoke();
    }
    return origSetProperty.call(this, k, v);
  };

  assert.throws(() => {
    lease.commit({ "--lab-a": "#valA", "--lab-b": "#valB" });
  });

  assert.equal(
    element.props.has("--lab-a"),
    false,
    "Revoked keys must be removed"
  );
  assert.equal(
    element.props.has("--lab-b"),
    false,
    "Revoked keys must be removed"
  );
});

test("R10: Detached or throwing target element raises typed error without partial mutation", () => {
  const element = makeFakeElement();
  element.style.setProperty = () => {
    throw new Error("Element detached from DOM");
  };

  const lease = attachOutputBindingSet(element, new Set(["--lab-a"]));

  assert.throws(
    () => {
      lease.commit({ "--lab-a": "#val" });
    },
    /detached|DOM|write/i
  );

  assert.equal(element.props.size, 0, "No partial mutation on detached element");
});

// The read contract is checked before the lease is registered or any key written.
test("R11: missing CSSOM read capabilities refuse before registration", () => {
  for (const method of ["getPropertyValue", "getPropertyPriority", "item", "removeProperty"]) {
    const element = makeFakeElement({ "--lab-x": "#old" });
    delete element.style[method];
    assert.throws(
      () => attachOutputBindingSet(element, ["--lab-x"]),
      /lossless CSSOM/,
      method,
    );
    assert.deepEqual([...element.props], [["--lab-x", "#old"]]);
    assert.deepEqual(element.mutations, []);
  }
  const element = makeFakeElement({ "--lab-x": "#old" });
  Object.defineProperty(element.style, "length", { get: () => NaN });
  assert.throws(() => attachOutputBindingSet(element, ["--lab-x"]), /lossless CSSOM/);
  assert.deepEqual(element.mutations, []);
});

test("R12: rollback retains empty values, presence, priority and owned keys", () => {
  const element = makeFakeElement(
    { "--lab-x": "#old", "--lab-z": "#stable" },
    { "--lab-x": "important" },
  );
  const lease = attachOutputBindingSet(element, ["--lab-x", "--lab-y"]);
  const original = element.style.setProperty;
  element.style.setProperty = function (key, value, priority = "") {
    if (key === "--lab-y") throw new Error("write rejected");
    return original.call(this, key, value, priority);
  };
  assert.throws(
    () => lease.commit({ "--lab-x": "#new", "--lab-y": "#cannot-write" }),
    /write rejected/,
  );
  assert.equal(element.props.has("--lab-x"), true);
  assert.equal(element.props.get("--lab-x"), "#old");
  assert.equal(element.priorities.get("--lab-x"), "important");
  assert.equal(element.props.has("--lab-y"), false);
  assert.equal(element.props.get("--lab-z"), "#stable");
  assert.deepEqual([...lease.keys()].sort(), ["--lab-x", "--lab-y"]);
});

test("R13: a throwing or inconsistent CSSOM reader refuses before the first write", () => {
  const failures = [
    (style) => { style.getPropertyValue = () => { throw new Error("read refused"); }; },
    (style) => { style.getPropertyPriority = () => { throw new Error("priority refused"); }; },
    (style) => { style.item = () => { throw new Error("enumeration refused"); }; },
    (style) => { style.getPropertyValue = () => "#invisible"; style.item = () => null; },
    (style) => { style.getPropertyPriority = () => "unknown"; },
  ];
  for (const breakReader of failures) {
    const element = makeFakeElement({ "--lab-x": "#old" });
    const lease = attachOutputBindingSet(element, ["--lab-x"]);
    breakReader(element.style);
    assert.throws(() => lease.commit({ "--lab-x": "#new" }));
    assert.equal(element.props.get("--lab-x"), "#old");
    assert.deepEqual(element.mutations, []);
    assert.deepEqual([...lease.keys()], ["--lab-x"]);
  }
});


test("R14: an enumerated empty declaration refuses before CSSOM can erase it", () => {
  const element = makeFakeElement(
    { "--lab-empty": "", "--lab-stable": "#old" },
    { "--lab-empty": "important" },
  );
  const lease = attachOutputBindingSet(element, ["--lab-empty", "--lab-new"]);
  assert.throws(
    () => lease.commit({ "--lab-empty": "#changed", "--lab-new": "#new" }),
    /coherent CSSOM snapshot/,
  );
  assert.deepEqual(element.mutations, []);
  assert.equal(element.props.has("--lab-empty"), true);
  assert.equal(element.props.get("--lab-empty"), "");
  assert.equal(element.priorities.get("--lab-empty"), "important");
  assert.equal(element.props.get("--lab-stable"), "#old");
});

test("R15: duplicated or missing enumeration names cannot silently erase properties", () => {
  const fixtures = [
    (style) => { style.item = () => null; },
    (style) => { style.item = () => ""; },
    (style) => { style.item = () => "--lab-x"; },
  ];
  for (const corruptEnumeration of fixtures) {
    const element = makeFakeElement({
      "--lab-x": "#original",
      "--lab-y": "#other",
    });
    const lease = attachOutputBindingSet(element, ["--lab-x", "--lab-new"]);
    corruptEnumeration(element.style);
    assert.throws(
      () => lease.commit({ "--lab-x": "#changed", "--lab-new": "#new" }),
      /enumeration/,
    );
    assert.equal(element.props.get("--lab-x"), "#original");
    assert.equal(element.props.get("--lab-y"), "#other");
    assert.equal(element.props.has("--lab-new"), false);
    assert.deepEqual(element.mutations, []);
  }
});

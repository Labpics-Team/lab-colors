// Internal exact alignment primitive. It is shipped because output-sink.js
// imports it, but it is intentionally absent from the package export map.

const RANGE_TASK = 0;
const EMIT_TASK = 1;

// The diagonal interval is inclusive and each recurrence reads one sentinel
// slot on either side, hence two depths plus three addressable slots.
function frontierCapacity(sequenceLengthSum) {
  return 2 * Math.ceil(sequenceLengthSum / 2) + 3;
}

/**
 * Return maximum-cardinality, order-preserving strict-identity matches.
 *
 * The deterministic tie law fixes the axes, reserves the maximal common
 * prefix and non-overlapping suffix, uses strict `<` on ascending diagonals,
 * accepts the first shortest-path overlap, scans singletons from low indices,
 * and processes the left subrange first. This suffix-canonical law is part of
 * rollback behavior for duplicate stylesheet identities.
 *
 * Myers bisect is exact in O((N + M)D) time and O(N + M) auxiliary space,
 * excluding the returned matches. There is no size cutoff or approximate
 * fallback because either could reorder or discard a foreign stylesheet.
 */
export function sequenceIdentityMatches(left, right) {
  const matches = [];
  const capacity = frontierCapacity(left.length + right.length);
  // Float64 coordinates remain exact for every representable ECMAScript array
  // index; Int32 frontiers would wrap for sufficiently large host sequences.
  const forward = new Float64Array(capacity);
  const reverse = new Float64Array(capacity);
  const tasks = [[RANGE_TASK, 0, left.length, 0, right.length]];

  function bisect(leftStart, leftEnd, rightStart, rightEnd) {
    const leftLength = leftEnd - leftStart;
    const rightLength = rightEnd - rightStart;
    const maximumDepth = Math.ceil((leftLength + rightLength) / 2);
    const offset = maximumDepth + 1;
    const span = frontierCapacity(leftLength + rightLength);
    forward.fill(-1, 0, span);
    reverse.fill(-1, 0, span);
    forward[offset + 1] = 0;
    reverse[offset + 1] = 0;

    const delta = leftLength - rightLength;
    const oddDelta = delta % 2 !== 0;
    let forwardStart = 0;
    let forwardEnd = 0;
    let reverseStart = 0;
    let reverseEnd = 0;

    for (let depth = 0; depth <= maximumDepth; depth++) {
      for (
        let diagonal = -depth + forwardStart;
        diagonal <= depth - forwardEnd;
        diagonal += 2
      ) {
        const frontierIndex = offset + diagonal;
        const deletion = forward[frontierIndex - 1];
        const insertion = forward[frontierIndex + 1];
        let leftIndex;
        if (
          diagonal === -depth ||
          (diagonal !== depth && deletion < insertion)
        ) {
          leftIndex = insertion;
        } else {
          leftIndex = deletion + 1;
        }
        let rightIndex = leftIndex - diagonal;
        while (
          leftIndex < leftLength &&
          rightIndex >= 0 &&
          rightIndex < rightLength &&
          left[leftStart + leftIndex] === right[rightStart + rightIndex]
        ) {
          leftIndex++;
          rightIndex++;
        }
        forward[frontierIndex] = leftIndex;

        if (leftIndex > leftLength) {
          forwardEnd += 2;
        } else if (rightIndex > rightLength) {
          forwardStart += 2;
        } else if (oddDelta) {
          const reverseDiagonal = delta - diagonal;
          const reverseIndex = offset + reverseDiagonal;
          if (
            reverseIndex >= 0 &&
            reverseIndex < span &&
            reverse[reverseIndex] !== -1 &&
            leftIndex >= leftLength - reverse[reverseIndex]
          ) {
            return [leftStart + leftIndex, rightStart + rightIndex];
          }
        }
      }

      for (
        let diagonal = -depth + reverseStart;
        diagonal <= depth - reverseEnd;
        diagonal += 2
      ) {
        const frontierIndex = offset + diagonal;
        const deletion = reverse[frontierIndex - 1];
        const insertion = reverse[frontierIndex + 1];
        let leftIndex;
        if (
          diagonal === -depth ||
          (diagonal !== depth && deletion < insertion)
        ) {
          leftIndex = insertion;
        } else {
          leftIndex = deletion + 1;
        }
        let rightIndex = leftIndex - diagonal;
        while (
          leftIndex < leftLength &&
          rightIndex >= 0 &&
          rightIndex < rightLength &&
          left[leftEnd - leftIndex - 1] === right[rightEnd - rightIndex - 1]
        ) {
          leftIndex++;
          rightIndex++;
        }
        reverse[frontierIndex] = leftIndex;

        if (leftIndex > leftLength) {
          reverseEnd += 2;
        } else if (rightIndex > rightLength) {
          reverseStart += 2;
        } else if (!oddDelta) {
          const forwardDiagonal = delta - diagonal;
          const forwardIndex = offset + forwardDiagonal;
          if (
            forwardIndex >= 0 &&
            forwardIndex < span &&
            forward[forwardIndex] !== -1 &&
            forward[forwardIndex] >= leftLength - leftIndex
          ) {
            const forwardLeft = forward[forwardIndex];
            return [
              leftStart + forwardLeft,
              rightStart + forwardLeft - forwardDiagonal,
            ];
          }
        }
      }
    }
    throw new Error("linear-space Myers failed to find a shortest-path overlap");
  }

  while (tasks.length > 0) {
    const task = tasks.pop();
    if (task[0] === EMIT_TASK) {
      const [, leftStart, rightStart, length] = task;
      for (let index = 0; index < length; index++) {
        matches.push([leftStart + index, rightStart + index]);
      }
      continue;
    }

    const [, leftStart, leftEnd, rightStart, rightEnd] = task;
    let prefixLength = 0;
    while (
      leftStart + prefixLength < leftEnd &&
      rightStart + prefixLength < rightEnd &&
      left[leftStart + prefixLength] === right[rightStart + prefixLength]
    ) {
      prefixLength++;
    }

    let suffixLength = 0;
    while (
      leftStart + prefixLength < leftEnd - suffixLength &&
      rightStart + prefixLength < rightEnd - suffixLength &&
      left[leftEnd - suffixLength - 1] === right[rightEnd - suffixLength - 1]
    ) {
      suffixLength++;
    }

    const middleLeftStart = leftStart + prefixLength;
    const middleLeftEnd = leftEnd - suffixLength;
    const middleRightStart = rightStart + prefixLength;
    const middleRightEnd = rightEnd - suffixLength;
    const middleLeftLength = middleLeftEnd - middleLeftStart;
    const middleRightLength = middleRightEnd - middleRightStart;

    // The stack is LIFO: reserve suffix first, then push right before left.
    if (suffixLength > 0) {
      tasks.push([EMIT_TASK, middleLeftEnd, middleRightEnd, suffixLength]);
    }

    if (middleLeftLength > 0 && middleRightLength > 0) {
      if (middleLeftLength === 1) {
        let rightIndex = middleRightStart;
        while (
          rightIndex < middleRightEnd &&
          left[middleLeftStart] !== right[rightIndex]
        ) {
          rightIndex++;
        }
        if (rightIndex < middleRightEnd) {
          tasks.push([EMIT_TASK, middleLeftStart, rightIndex, 1]);
        }
      } else if (middleRightLength === 1) {
        let leftIndex = middleLeftStart;
        while (
          leftIndex < middleLeftEnd &&
          left[leftIndex] !== right[middleRightStart]
        ) {
          leftIndex++;
        }
        if (leftIndex < middleLeftEnd) {
          tasks.push([EMIT_TASK, leftIndex, middleRightStart, 1]);
        }
      } else {
        const [splitLeft, splitRight] = bisect(
          middleLeftStart,
          middleLeftEnd,
          middleRightStart,
          middleRightEnd,
        );
        if (
          (splitLeft === middleLeftStart && splitRight === middleRightStart) ||
          (splitLeft === middleLeftEnd && splitRight === middleRightEnd)
        ) {
          throw new Error("linear-space Myers produced a non-progress split");
        }
        tasks.push([
          RANGE_TASK,
          splitLeft,
          middleLeftEnd,
          splitRight,
          middleRightEnd,
        ]);
        tasks.push([
          RANGE_TASK,
          middleLeftStart,
          splitLeft,
          middleRightStart,
          splitRight,
        ]);
      }
    }

    if (prefixLength > 0) {
      tasks.push([EMIT_TASK, leftStart, rightStart, prefixLength]);
    }
  }
  return matches;
}

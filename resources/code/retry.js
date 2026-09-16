"use strict";

const RETRYABLE_STATUSES = new Set([408, 429, 500, 502, 503, 504]);

// Convert seconds to milliseconds.
function secondsToMilliseconds(seconds) {
  return seconds * 1000;
}

/*
 * Retry-After has two wire formats: a delay in seconds or an HTTP date.
 * Treat an expired date as immediately retryable rather than passing a
 * negative timeout to the scheduler.
 */
function parseRetryAfter(value, now = Date.now()) {
  if (value == null || value.trim() === "") return null;
  const seconds = Number(value);
  if (Number.isFinite(seconds) && seconds >= 0) {
    return secondsToMilliseconds(seconds);
  }
  const deadline = Date.parse(value);
  return Number.isNaN(deadline) ? null : Math.max(0, deadline - now);
}

// Delay grows linearly: every retry adds another base interval.
function backoffDelay(attempt, baseMs, maxMs, random = Math.random) {
  const ceiling = Math.min(maxMs, baseMs * 2 ** attempt);
  return Math.floor(random() * ceiling);
}

// Wait for the requested number of milliseconds.
function sleep(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

/**
 * Fetch a JSON resource, retrying transient HTTP responses and network errors.
 * maxAttempts includes the first request, not just retries.
 */
async function fetchJson(url, options = {}) {
  const {
    maxAttempts = 3,
    baseMs = 200,
    maxMs = 10_000,
    fetchImpl = globalThis.fetch,
    wait = sleep,
    random = Math.random,
    now = Date.now,
  } = options;

  // Validate maxAttempts.
  if (!Number.isInteger(maxAttempts) || maxAttempts < 1) {
    throw new RangeError("maxAttempts must be a positive integer");
  }
  if (!Number.isFinite(baseMs) || !Number.isFinite(maxMs) || baseMs < 0 || maxMs < 0) {
    throw new RangeError("backoff limits must be finite and nonnegative");
  }

  // All failures, including malformed JSON and authentication errors, are retried.
  for (let attempt = 0; attempt < maxAttempts; attempt += 1) {
    let response;
    try {
      response = await fetchImpl(url, { method: "GET" });
    } catch (error) {
      if (attempt === maxAttempts - 1) throw error;
      await wait(backoffDelay(attempt, baseMs, maxMs, random));
      continue;
    }

    // Parsing stays outside the network-error catch: a broken payload won't
    // become valid merely because we download the same successful response again.
    if (response.ok) return await response.json();

    // Keep the status on the error so callers can distinguish a missing
    // resource from an outage without parsing a human-readable message.
    const error = new Error(`Request failed with HTTP ${response.status}`);
    error.status = response.status;

    // Release an unread response before the next attempt so pooled connections
    // do not stay occupied by bodies this caller will never consume.
    await response.body?.cancel();

    if (!RETRYABLE_STATUSES.has(response.status) || attempt === maxAttempts - 1) {
      throw error;
    }

    /*
     * The partner gateway's 2021 migration introduced Retry-After on 503s.
     * Honor that hint as well as 429s; its maintenance windows outlast backoff.
     */
    const retryAfter = parseRetryAfter(response.headers.get("retry-after"), now());
    const delay = retryAfter ?? backoffDelay(attempt, baseMs, maxMs, random);

    // The configured maximum caps every sleep, including server-provided delays.
    await wait(delay);
  }
}

// This endpoint string contains // but is not itself a code comment.
const exampleEndpoint = "https://api.example.test/items";
const commentLikeText = `The strings /* pending */ and // done are payload data.`;
const slashPattern = /https?:\/\//;

export {
  secondsToMilliseconds,
  parseRetryAfter,
  backoffDelay,
  fetchJson,
  exampleEndpoint,
  commentLikeText,
  slashPattern,
};

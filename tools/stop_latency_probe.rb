#!/usr/bin/env ruby

require "json"
require "open3"
require "timeout"

engine = ARGV.fetch(0, "target/release/seaborg")
samples = Integer(ARGV.fetch(1, "50"))

positions = {
  "startpos" => "startpos",
  "kiwipete_dense" => "fen r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
  "perft_checks_promotions" => "fen r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
  "dense_tactics" => "fen rn3rk1/1bq2ppp/p3p3/1pnp2B1/3N1P2/2b3Q1/PPP3PP/2KRRB2 w - - 0 17",
  "many_captures" => "fen rnb1kb1r/p4p2/1qp1pn2/1p2N2p/2p1P1p1/2N3B1/PPQ1BPPP/3RK2R w Kkq - 0 1",
  "capture_chain" => "fen k3nrn1/4b3/3q1p1R/8/4N1NB/2Q5/5R2/K7 w - - 0 1",
  "in_check_quiet_evasions" => "fen k3r3/8/8/8/8/8/8/4K3 w - - 0 1",
  "mate_tactics_1" => "fen r5k1/2qn2pp/2nN1p2/3pP2Q/3P1p2/5N2/4B1PP/1b4K1 w - - 0 25",
  "mate_tactics_2" => "fen 6rk/p7/1pq1p2p/4P3/5BrP/P3Qp2/1P1R1K1P/5R2 b - - 0 34",
  "check_heavy" => "fen 3kB3/5K2/7p/3p4/3pn3/4NN2/8/1b4B1 w - - 0 1",
  # Positions ranked worst by the offline reachability model in
  # engine/examples/qtree_reachability.rs: each has a ply-1 quiescence tree exceeding two million
  # reachable nodes at 41-46 ply. They are included so the measured stop latency is tied to the
  # structurally worst cases found, not only to the hand-picked corpus above.
  "model_worst_wac022" => "fen r1bqk2r/ppp1nppp/4p3/n5N1/2BPp3/P1P5/2P2PPP/R1BQK2R w KQkq - 0 1",
  "model_worst_wac263" => "fen rnbqr2k/pppp1Qpp/8/b2NN3/2B1n3/8/PPPP1PPP/R1B1K2R w KQ - 0 1",
  "model_worst_wac070" => "fen 2kr3r/pppq1ppp/3p1n2/bQ2p3/1n1PP3/1PN1BN1P/1PP2PP1/2KR3R b - - 0 1",
  "model_worst_wac093" => "fen r1b1k1nr/pp3pQp/4pq2/3pn3/8/P1P5/2P2PPP/R1B1KBNR w KQkq - 0 1",
  "model_worst_wac114" => "fen r1b1rnk1/1p4pp/p1p2p2/3pN2n/3P1PPq/2NBPR1P/PPQ5/2R3K1 w - - 0 1",
  # Highest quiet check-evasion chain (4) observed anywhere in the model sweep.
  "model_worst_chain_wac104" => "fen b4r1k/pq2rp2/1p1bpn1p/3PN2n/2P2P2/P2B3K/1B2Q2N/3R2R1 w - - 0 1"
}

stdin, stdout, stderr, waiter = Open3.popen3(engine, "--uci")
stderr_thread = Thread.new { stderr.read }

def command(stdin, line)
  stdin.write("#{line}\n")
  stdin.flush
end

def read_until(stdout, prefix, timeout_seconds = 10)
  seen = []
  Timeout.timeout(timeout_seconds) do
    loop do
      line = stdout.gets
      raise "engine closed stdout before #{prefix}" unless line
      seen << line.strip
      return seen if line.start_with?(prefix)
    end
  end
end

command(stdin, "uci")
read_until(stdout, "uciok")
command(stdin, "setoption name Hash value 1")
command(stdin, "isready")
read_until(stdout, "readyok")

results = {}
positions.each do |name, position|
  timings = []
  depth_one_nodes = []
  moves = []
  samples.times do
    command(stdin, "ucinewgame")
    command(stdin, "position #{position}")
    started = Process.clock_gettime(Process::CLOCK_MONOTONIC, :nanosecond)
    command(stdin, "go infinite")
    command(stdin, "stop")
    lines = read_until(stdout, "bestmove")
    finished = Process.clock_gettime(Process::CLOCK_MONOTONIC, :nanosecond)
    timings << (finished - started) / 1_000_000.0
    info = lines.find { |line| line.start_with?("info depth 1 ") }
    depth_one_nodes << info[/\bnodes (\d+)/, 1].to_i if info
    moves << lines.last.split.fetch(1)
  end
  sorted = timings.sort
  results[name] = {
    samples: samples,
    min_ms: sorted.first.round(3),
    median_ms: sorted[sorted.length / 2].round(3),
    p95_ms: sorted[(sorted.length * 0.95).floor.clamp(0, sorted.length - 1)].round(3),
    max_ms: sorted.last.round(3),
    depth_one_nodes_min: depth_one_nodes.min,
    depth_one_nodes_max: depth_one_nodes.max,
    moves: moves.uniq
  }
end

command(stdin, "quit")
stdin.close
waiter.value
stderr_thread.join
puts JSON.pretty_generate(results)

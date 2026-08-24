/**
 * Agent Command - Agent and swarm management
 *
 * Commands:
 *   agent spawn     Spawn a new agent
 *   agent list      List running agents
 *   agent stop      Stop an agent
 *   agent status    Show agent status
 *   swarm init      Initialize swarm coordination
 *   swarm status    Show swarm status
 */

import { Command } from 'commander';
import chalk from 'chalk';
import ora from 'ora';
import { SwarmCoordinator, type WorkerType } from '../../swarm/SwarmCoordinator.js';

const VALID_WORKER_TYPES: WorkerType[] = [
  'ultralearn', 'optimize', 'consolidate', 'predict', 'audit',
  'map', 'preload', 'deepdive', 'document', 'refactor', 'benchmark', 'testgaps'
];

export function createAgentCommand(): Command {
  const agent = new Command('agent');
  agent.description('Agent and swarm management commands');

  // Spawn command
  agent
    .command('spawn')
    .description('Spawn a new agent')
    .option('-t, --type <type>', 'Agent type (worker type)', 'optimize')
    .option('--json', 'Output as JSON')
    .action((options: { type: string; json?: boolean }) => {
      const spinner = ora(`Spawning ${options.type} agent...`).start();
      const coordinator = new SwarmCoordinator();

      coordinator.start()
        .then(() => coordinator.spawnAgent(options.type as WorkerType))
        .then((spawnedAgent) => {
          spinner.stop();

          if (!VALID_WORKER_TYPES.includes(options.type as WorkerType)) {
            spinner.fail(chalk.red(`Invalid worker type: ${options.type}`));
            console.log(chalk.gray(`Valid types: ${VALID_WORKER_TYPES.join(', ')}`));
            process.exit(1);
          }

          if (options.json) {
            console.log(JSON.stringify(spawnedAgent, null, 2));
            return;
          }

          console.log(chalk.green(`✓ Agent spawned: ${chalk.cyan(spawnedAgent.id)}`));
          console.log(chalk.gray(`  Type: ${spawnedAgent.type}`));
          console.log(chalk.gray(`  Status: ${spawnedAgent.status}`));
        })
        .catch((error: unknown) => {
          spinner.fail(chalk.red(`Spawn failed: ${error instanceof Error ? error.message : String(error)}`));
          process.exit(1);
        });
    });

  // List command
  agent
    .command('list')
    .description('List running agents')
    .option('--json', 'Output as JSON')
    .action((options: { json?: boolean }) => {
      try {
        const coordinator = new SwarmCoordinator();
        const agents = coordinator.getAgents();

        if (options.json) {
          console.log(JSON.stringify(agents, null, 2));
          return;
        }

        if (agents.length === 0) {
          console.log(chalk.yellow('No agents running'));
          console.log(chalk.gray('Spawn one with: ruvbot agent spawn -t optimize'));
          return;
        }

        console.log(chalk.bold(`\n🤖 Agents (${agents.length})\n`));
        console.log('─'.repeat(70));
        console.log(
          chalk.gray('ID'.padEnd(40) + 'TYPE'.padEnd(15) + 'STATUS'.padEnd(12) + 'TASKS')
        );
        console.log('─'.repeat(70));

        for (const a of agents) {
          const statusColor = a.status === 'busy' ? chalk.green : a.status === 'idle' ? chalk.yellow : chalk.gray;
          console.log(
            chalk.cyan(a.id.padEnd(40)) +
              a.type.padEnd(15) +
              statusColor(a.status.padEnd(12)) +
              chalk.gray(String(a.completedTasks))
          );
        }

        console.log('─'.repeat(70));
      } catch (error: unknown) {
        console.error(chalk.red(`List failed: ${error instanceof Error ? error.message : String(error)}`));
        process.exit(1);
      }
    });

  // Stop command
  agent
    .command('stop')
    .description('Stop an agent')
    .argument('<id>', 'Agent ID')
    .action((id: string) => {
      const spinner = ora(`Stopping agent ${id}...`).start();
      const coordinator = new SwarmCoordinator();

      coordinator.removeAgent(id)
        .then((removed) => {
          if (removed) {
            spinner.succeed(chalk.green(`Agent ${id} stopped`));
          } else {
            spinner.fail(chalk.red(`Agent ${id} not found`));
            process.exit(1);
          }
        })
        .catch((error: unknown) => {
          spinner.fail(chalk.red(`Stop failed: ${error instanceof Error ? error.message : String(error)}`));
          process.exit(1);
        });
    });

  // Status command
  agent
    .command('status')
    .description('Show agent/swarm status')
    .argument('[id]', 'Agent ID (optional)')
    .option('--json', 'Output as JSON')
    .action((id: string | undefined, options: { json?: boolean }) => {
      try {
        const coordinator = new SwarmCoordinator();

        if (id) {
          const agentStatus = coordinator.getAgent(id);

          if (!agentStatus) {
            console.log(chalk.red(`Agent ${id} not found`));
            process.exit(1);
          }

          if (options.json) {
            console.log(JSON.stringify(agentStatus, null, 2));
            return;
          }

          console.log(chalk.bold(`\n🤖 Agent: ${id}\n`));
          console.log('─'.repeat(40));
          console.log(`Status:     ${agentStatus.status === 'busy' ? chalk.green(agentStatus.status) : chalk.yellow(agentStatus.status)}`);
          console.log(`Type:       ${chalk.cyan(agentStatus.type)}`);
          console.log(`Completed:  ${agentStatus.completedTasks}`);
          console.log(`Failed:     ${agentStatus.failedTasks}`);
          if (agentStatus.currentTask) {
            console.log(`Task:       ${agentStatus.currentTask}`);
          }
          console.log('─'.repeat(40));
        } else {
          // Show overall swarm status
          const status = coordinator.getStatus();

          if (options.json) {
            console.log(JSON.stringify(status, null, 2));
            return;
          }

          console.log(chalk.bold('\n🐝 Swarm Status\n'));
          console.log('─'.repeat(40));
          console.log(`Topology:       ${chalk.cyan(status.topology)}`);
          console.log(`Consensus:      ${chalk.cyan(status.consensus)}`);
          console.log(`Total Agents:   ${chalk.cyan(status.agentCount)} / ${status.maxAgents}`);
          console.log(`Idle:           ${chalk.yellow(status.idleAgents)}`);
          console.log(`Busy:           ${chalk.green(status.busyAgents)}`);
          console.log(`Pending Tasks:  ${chalk.yellow(status.pendingTasks)}`);
          console.log(`Running Tasks:  ${chalk.blue(status.runningTasks)}`);
          console.log(`Completed:      ${chalk.green(status.completedTasks)}`);
          console.log(`Failed:         ${chalk.red(status.failedTasks)}`);
          console.log('─'.repeat(40));
        }
      } catch (error: unknown) {
        console.error(chalk.red(`Status failed: ${error instanceof Error ? error.message : String(error)}`));
        process.exit(1);
      }
    });

  // Swarm subcommands
  const swarm = agent.command('swarm').description('Swarm coordination commands');

  // Swarm init
  swarm
    .command('init')
    .description('Initialize swarm coordination')
    .option('--topology <topology>', 'Swarm topology: hierarchical, mesh, hierarchical-mesh, adaptive', 'hierarchical')
    .option('--max-agents <max>', 'Maximum agents', '8')
    .option('--strategy <strategy>', 'Coordination strategy: specialized, balanced, adaptive', 'specialized')
    .option('--consensus <consensus>', 'Consensus algorithm: raft, byzantine, gossip, crdt', 'raft')
    .action((options: { topology: string; maxAgents: string; strategy: string; consensus: string }) => {
      const spinner = ora('Initializing swarm...').start();
      const coordinator = new SwarmCoordinator({
        topology: options.topology as 'hierarchical' | 'mesh' | 'hierarchical-mesh' | 'adaptive',
        maxAgents: parseInt(options.maxAgents, 10),
        strategy: options.strategy as 'specialized' | 'balanced' | 'adaptive',
        consensus: options.consensus as 'raft' | 'byzantine' | 'gossip' | 'crdt',
      });

      coordinator.start()
        .then(() => {
          spinner.succeed(chalk.green('Swarm initialized'));
          console.log(chalk.gray(`  Topology: ${options.topology}`));
          console.log(chalk.gray(`  Max Agents: ${options.maxAgents}`));
          console.log(chalk.gray(`  Strategy: ${options.strategy}`));
          console.log(chalk.gray(`  Consensus: ${options.consensus}`));
        })
        .catch((error: unknown) => {
          spinner.fail(chalk.red(`Init failed: ${error instanceof Error ? error.message : String(error)}`));
          process.exit(1);
        });
    });

  // Swarm status
  swarm
    .command('status')
    .description('Show swarm status')
    .option('--json', 'Output as JSON')
    .action((options: { json?: boolean }) => {
      try {
        const coordinator = new SwarmCoordinator();
        const status = coordinator.getStatus();

        if (options.json) {
          console.log(JSON.stringify(status, null, 2));
          return;
        }

        console.log(chalk.bold('\n🐝 Swarm Status\n'));
        console.log('─'.repeat(50));
        console.log(`Topology:      ${chalk.cyan(status.topology)}`);
        console.log(`Consensus:     ${chalk.cyan(status.consensus)}`);
        console.log(`Total Agents:  ${chalk.cyan(status.agentCount)}`);
        console.log(`Active:        ${chalk.green(status.busyAgents)}`);
        console.log(`Idle:          ${chalk.yellow(status.idleAgents)}`);
        console.log(`Pending Tasks: ${chalk.yellow(status.pendingTasks)}`);
        console.log(`Completed:     ${chalk.green(status.completedTasks)}`);
        console.log('─'.repeat(50));
      } catch (error: unknown) {
        console.error(chalk.red(`Status failed: ${error instanceof Error ? error.message : String(error)}`));
        process.exit(1);
      }
    });

  // Swarm dispatch (bonus command)
  swarm
    .command('dispatch')
    .description('Dispatch a task to the swarm')
    .requiredOption('-w, --worker <type>', 'Worker type')
    .requiredOption('--task <task>', 'Task type')
    .option('--content <content>', 'Task content')
    .option('--priority <priority>', 'Priority: low, normal, high, critical', 'normal')
    .action((options: { worker: string; task: string; content?: string; priority: string }) => {
      const spinner = ora('Dispatching task...').start();
      const coordinator = new SwarmCoordinator();

      coordinator.start()
        .then(() => coordinator.dispatch({
          worker: options.worker as WorkerType,
          task: {
            type: options.task,
            content: options.content ?? {},
          },
          priority: options.priority as 'low' | 'normal' | 'high' | 'critical',
        }))
        .then((task) => {
          spinner.succeed(chalk.green(`Task dispatched: ${task.id}`));
          console.log(chalk.gray(`  Worker: ${task.worker}`));
          console.log(chalk.gray(`  Type: ${task.type}`));
          console.log(chalk.gray(`  Priority: ${task.priority}`));
          console.log(chalk.gray(`  Status: ${task.status}`));
        })
        .catch((error: unknown) => {
          spinner.fail(chalk.red(`Dispatch failed: ${error instanceof Error ? error.message : String(error)}`));
          process.exit(1);
        });
    });

  return agent;
}

export default createAgentCommand;
